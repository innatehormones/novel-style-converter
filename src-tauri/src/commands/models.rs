use std::sync::{Arc, Mutex};
use std::time::Instant;

use nsc_core::ai::{AiProvider, ChatMessage, ChatRequest, OpenAiProvider, Role};
use nsc_core::db::Db;
use nsc_core::models::{ModelConfig, NewModelConfig};
use serde::{Deserialize, Serialize};
use tauri::State;

/// 默认列表(仅 `archived = 0`)。各 dialog 拉模型下拉用。
#[tauri::command]
pub fn list_models(db: State<'_, Arc<Mutex<Db>>>) -> Result<Vec<ModelConfig>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.model_configs().list(false).map_err(|e| e.to_string())
}

/// 含归档的列表 —— Models.vue 顶部“显示已归档”开关用。
#[tauri::command]
pub fn list_models_including_archived(db: State<'_, Arc<Mutex<Db>>>) -> Result<Vec<ModelConfig>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.model_configs().list(true).map_err(|e| e.to_string())
}

/// 软删:`archived = 1` + `api_key = ''`。行保留以便历史 tc / tn 引用解析。
#[tauri::command]
pub fn delete_model(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.model_configs().archive(id).map_err(|e| e.to_string())
}

/// 取消软删。注意:被抹掉的 `api_key` 不会自动恢复,用户需重新编辑保存。
#[tauri::command]
pub fn restore_model(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.model_configs().restore(id).map_err(|e| e.to_string())
}

/// 前端 wrapper 透传 snake_case payload,DTO 字段保持 Rust 原名。
#[derive(Debug, Deserialize)]
pub struct ModelConfigDto {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: Option<i32>,
    pub temperature: Option<f32>,
    pub concurrency: i32,
}

impl ModelConfigDto {
    fn into_new(self) -> NewModelConfig {
        NewModelConfig {
            name: self.name,
            base_url: self.base_url,
            api_key: self.api_key,
            model: self.model,
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            concurrency: self.concurrency,
        }
    }

    fn into_full(self) -> ModelConfig {
        ModelConfig {
            id: self.id,
            name: self.name,
            base_url: self.base_url,
            api_key: self.api_key,
            model: self.model,
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            concurrency: self.concurrency,
            // update 路径保留原 archived(0),不允许通过 upsert 改 archived。
            archived: 0,
        }
    }
}

/// 新建或更新 `ModelConfig`。`payload.id == 0` 走 insert(返回新 id);
/// 否则走 update(返回传入的 id)。`ModelConfigDto` 是手写 snake_case DTO
/// (后端 `#[serde(rename_all = "snake_case")]`),前端调用时内层字段保持 snake_case。
#[tauri::command]
pub fn upsert_model(db: State<'_, Arc<Mutex<Db>>>, payload: ModelConfigDto) -> Result<i64, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    if payload.id == 0 {
        let new = ModelConfigDto::into_new(payload);
        db.model_configs().insert(&new).map_err(|e| e.to_string())
    } else {
        let full = ModelConfigDto::into_full(payload);
        db.model_configs().update(&full).map_err(|e| e.to_string())?;
        Ok(full.id)
    }
}

/// `test_model` 返回结构化报告 —— 让 UI 完整展示 latency / token 计数 / 内容预览。
/// - `content_preview` 是响应前 200 字符(完整原文仍在后端 `provider.chat` 之后可扩展)。
/// - `error` 在失败时携带完整字符串(非 2xx / 空 choices / 缺 usage 都会写)。
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TestModelReport {
    pub model: String,
    pub base_url: String,
    pub latency_ms: i64,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
    pub content_preview: Option<String>,
    pub error: Option<String>,
}

/// 实际发起一次 `chat` 调用(payload 的模型 + 用户消息 "ping")验证连通性。
/// **不抛错**:失败时把错误字符串塞进 `TestModelReport.error`,UI 可正常展示失败详情。
/// 成功时填 latency / tokens / content_preview。
#[tauri::command]
pub async fn test_model(payload: ModelConfigDto) -> Result<TestModelReport, String> {
    let started = Instant::now();
    let report = match OpenAiProvider::new(payload.base_url.clone(), payload.api_key.clone()) {
        Err(e) => TestModelReport {
            model: payload.model,
            base_url: payload.base_url,
            latency_ms: started.elapsed().as_millis() as i64,
            tokens_in: None,
            tokens_out: None,
            content_preview: None,
            error: Some(format!("create provider failed: {e}")),
        },
        Ok(provider) => match provider
            .chat(ChatRequest {
                model: payload.model.clone(),
                messages: vec![ChatMessage { role: Role::User, content: "ping".into() }],
                temperature: payload.temperature,
                max_tokens: payload.max_tokens,
            })
            .await
        {
            Ok(resp) => {
                let preview: String = resp.content.chars().take(200).collect();
                TestModelReport {
                    model: payload.model,
                    base_url: payload.base_url,
                    latency_ms: started.elapsed().as_millis() as i64,
                    tokens_in: Some(resp.tokens_in),
                    tokens_out: Some(resp.tokens_out),
                    content_preview: Some(preview),
                    error: None,
                }
            }
            Err(e) => TestModelReport {
                model: payload.model,
                base_url: payload.base_url,
                latency_ms: started.elapsed().as_millis() as i64,
                tokens_in: None,
                tokens_out: None,
                content_preview: None,
                error: Some(e.to_string()),
            },
        },
    };
    Ok(report)
}
