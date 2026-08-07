use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;

use crate::ai::{AiProvider, ChatMessage, ChatRequest, Role};
use crate::error::{Error, Result};
use crate::models::{AiCallStatus, Chapter, ModelConfig, Prompt, TransformationNovel};
use crate::prompts::{render, PromptContext};
use crate::recorder::{AiCallEvent, AiCallRecorder};

pub struct TransformRequest {
    pub transformation_id: i64,
    pub chapter: Chapter,
    /// 章节正文切片(由 `queue.rs` 从 `chapters.body` 取出)。
    pub chapter_content: String,
    pub novel_context: TransformationNovelContext,
    pub prompt: Prompt,
    pub model_config: ModelConfig,
}

pub struct TransformationNovelContext {
    pub transformation_novel: TransformationNovel,
    /// 邻章原文片段 —— 同样由 `queue.rs` 取出,Vec 元素是 `(title, content)` 对。
    pub prev_original: Vec<(String, String)>,
    /// 邻章已转换正文 —— 元素是 (title, content) 对;queue.rs 负责 join
    /// workflow_result_chapters 拿真内容。
    pub prev_transformed: Vec<(String, String)>,
    pub next_original: Vec<(String, String)>,
}

pub struct TransformOutcome {
    pub result_content: String,
    pub tokens_in: i32,
    pub tokens_out: i32,
}

/// 把 prompt + 上下文渲染成 chat 请求并发给 `AiProvider` 的抽象。
/// `JobQueue` 通过 `Box<dyn Transformer>` 持有实例,所以 `Transformer` 要求 `Send + Sync`。
#[async_trait]
pub trait Transformer: Send + Sync {
    async fn transform(&self, req: TransformRequest) -> Result<TransformOutcome>;
}

/// `Transformer` 的默认实现:渲染 prompt → 调 `AiProvider::chat` → 透传结果。
/// **owns** `Arc<dyn AiProvider>` + `Arc<dyn AiCallRecorder>`,
/// 这样能装进 `Box<dyn Transformer>` 且与 worker 内部 `ProviderCache` 共享 provider 句柄。
///
/// ## Recorder 集成
/// - `transform()` 始终 record 一次,不管成功失败(失败路径也写 error + status=failed)。
/// - `record()` 不阻塞:channel 满 → drop new。
/// - **不**把 recorder 失败往上传 —— transform 业务成功了就算"账没记上",业务结果不受影响。
pub struct DefaultTransformer {
    pub ai: Arc<dyn AiProvider>,
    pub recorder: Arc<dyn AiCallRecorder>,
}

#[async_trait]
impl Transformer for DefaultTransformer {
    async fn transform(&self, req: TransformRequest) -> Result<TransformOutcome> {
        let ctx = PromptContext {
            transformation_novel: &req.novel_context.transformation_novel,
            chapter: &req.chapter,
            chapter_content: &req.chapter_content,
            prev_original: &req.novel_context.prev_original,
            prev_transformed: &req.novel_context.prev_transformed,
            next_original: &req.novel_context.next_original,
            kind: req.prompt.kind,
        };
        let rendered = render(&req.prompt.template, &ctx);
        let system_full = rendered.system.clone().unwrap_or_default();
        let user_full = rendered.user.clone();
        let estimated_tokens_in = ((system_full.chars().count() + user_full.chars().count()) / 2) as i32;
        let mut messages = Vec::with_capacity(if !system_full.is_empty() { 2 } else { 1 });
        if !system_full.is_empty() {
            messages.push(ChatMessage { role: Role::System, content: system_full.clone() });
        }
        messages.push(ChatMessage { role: Role::User, content: user_full.clone() });
        let chat_req = ChatRequest {
            model: req.model_config.model.clone(),
            messages,
            temperature: req.model_config.temperature,
            max_tokens: req.model_config.max_tokens,
        };

        let started = Instant::now();
        let ai_result = self.ai.chat(chat_req).await;
        let latency_ms = started.elapsed().as_millis() as i64;

        // Recorder event 拼装 —— 不管 ai_result 成功失败,始终 record 一次。
        // 注意:这里不能 `?` 提前返回(否则失败路径不会 record)。
        let (status, response_full, actual_in, actual_out, error_msg, outcome) = match &ai_result {
            Ok(r) => (
                AiCallStatus::Success,
                r.content.clone(),
                Some(r.tokens_in),
                Some(r.tokens_out),
                None,
                Ok(TransformOutcome {
                    result_content: r.content.clone(),
                    tokens_in: r.tokens_in,
                    tokens_out: r.tokens_out,
                }),
            ),
            Err(e) => (
                AiCallStatus::Failed,
                String::new(),
                None,
                None,
                Some(e.to_string()),
                Err(Error::Ai(e.to_string())),
            ),
        };
        self.recorder.record(AiCallEvent {
            business: crate::models::AiCallBusiness::TransformChapter,
            context_type: Some("transformation_chapter".into()),
            context_id: Some(req.transformation_id),
            model_config_id: Some(req.model_config.id),
            model_name: req.model_config.model.clone(),
            base_url: req.model_config.base_url.clone(),
            temperature: req.model_config.temperature,
            max_tokens: req.model_config.max_tokens,
            system_full: system_full.clone(),
            user_full: user_full.clone(),
            estimated_tokens_in: Some(estimated_tokens_in),
            actual_tokens_in: actual_in,
            actual_tokens_out: actual_out,
            status,
            response_full,
            latency_ms,
            error: error_msg,
        });

        outcome
    }
}
