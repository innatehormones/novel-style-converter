use std::sync::{Arc, Mutex};

use nsc_core::ai::{AiProvider, ChatMessage, ChatRequest, OpenAiProvider, Role};
use nsc_core::db::Db;
use nsc_core::models::{ModelConfig, NewModelConfig};
use serde::Deserialize;
use tauri::State;

#[tauri::command]
pub fn list_models(db: State<'_, Arc<Mutex<Db>>>) -> Result<Vec<ModelConfig>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.model_configs().list().map_err(|e| e.to_string())
}

// 前端 wrapper 透传 snake_case payload,DTO 字段保持 Rust 原名。
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
        }
    }
}

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

#[tauri::command]
pub fn delete_model(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.model_configs().delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_model(payload: ModelConfigDto) -> Result<String, String> {
    let provider = OpenAiProvider::new(payload.base_url, payload.api_key)
        .map_err(|e| e.to_string())?;
    let response = provider
        .chat(ChatRequest {
            model: payload.model,
            messages: vec![ChatMessage { role: Role::User, content: "ping".into() }],
            temperature: payload.temperature,
            max_tokens: payload.max_tokens,
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(response.content)
}
