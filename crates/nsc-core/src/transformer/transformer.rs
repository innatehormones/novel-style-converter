use std::sync::Arc;

use async_trait::async_trait;

use crate::ai::{AiProvider, ChatMessage, ChatRequest, Role};
use crate::error::Result;
use crate::models::{Chapter, ModelConfig, Prompt, TransformationChapter, TransformationNovel};
use crate::prompts::{render, PromptContext};

pub struct TransformRequest {
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
    pub prev_transformed: Vec<TransformationChapter>,
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
/// **owns** `Arc<dyn AiProvider>`,这样能装进 `Box<dyn Transformer>` 且与 worker 内部
/// `ProviderCache` 共享 provider 句柄(避免每次 job 重建 reqwest 客户端)。
pub struct DefaultTransformer { pub ai: Arc<dyn AiProvider> }

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
        };
        let user_content = render(&req.prompt.template, &ctx)?;
        let chat_req = ChatRequest {
            model: req.model_config.model.clone(),
            messages: vec![ChatMessage {
                role: Role::User, content: user_content,
            }],
            temperature: req.model_config.temperature,
            max_tokens: req.model_config.max_tokens,
        };
        let r = self.ai.chat(chat_req).await?;
        Ok(TransformOutcome {
            result_content: r.content,
            tokens_in: r.tokens_in,
            tokens_out: r.tokens_out,
        })
    }
}
