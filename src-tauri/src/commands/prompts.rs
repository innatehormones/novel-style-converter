use std::sync::{Arc, Mutex};

use nsc_core::db::Db;
use nsc_core::models::{Prompt, PromptKind};
use serde::Deserialize;
use tauri::State;

#[tauri::command]
pub fn list_prompts(db: State<'_, Arc<Mutex<Db>>>) -> Result<Vec<Prompt>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.prompts().list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_prompt(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<Prompt, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.prompts()
        .get(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("prompt {id} 不存在"))
}

/// `upsert_prompt` 入参。`id == 0` 走 insert(返回新 id);>0 走 update(返回传入 id)。
/// 内层 DTO 没有 `#[serde(rename_all = "snake_case")]`(字段全单词),前端按字段名原样发。
/// `kind` 用 `PromptKind`,后端 `#[serde(rename_all = "snake_case")]` 自动映射 `"compress"` / `"style"`。
#[derive(Debug, Deserialize)]
pub struct PromptInput {
    pub id: i64,
    pub name: String,
    pub kind: PromptKind,
    pub template: String,
}

#[tauri::command]
pub fn upsert_prompt(
    db: State<'_, Arc<Mutex<Db>>>,
    payload: PromptInput,
) -> Result<i64, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    if payload.id == 0 {
        let new = Prompt {
            id: 0,
            name: payload.name,
            kind: payload.kind,
            template: payload.template,
            is_builtin: false,
        };
        db.prompts().insert(&new).map_err(|e| e.to_string())
    } else {
        // 更新前先读现有的 is_builtin:UI 不会让 builtin 进 update 流程,
        // 但万一收到 builtin 的 update,也保留 builtin 标记,不静默改写。
        let existing = db
            .prompts()
            .get(payload.id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("prompt {} 不存在", payload.id))?;
        let updated = Prompt {
            id: existing.id,
            name: payload.name,
            kind: payload.kind,
            template: payload.template,
            is_builtin: existing.is_builtin,
        };
        db.prompts().update(&updated).map_err(|e| e.to_string())?;
        Ok(updated.id)
    }
}

#[tauri::command]
pub fn delete_prompt(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.prompts().delete(id).map_err(|e| e.to_string())
}

/// 统计 prompt 被 `transformation_chapters` 引用的次数。
/// 前端删除 prompt 前展示"被 N 个转换结果引用",N=0 不展示。
#[tauri::command]
pub fn count_transformation_chapters_by_prompt(
    db: State<'_, Arc<Mutex<Db>>>,
    prompt_id: i64,
) -> Result<i64, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.prompts()
        .count_by_prompt(prompt_id)
        .map_err(|e| e.to_string())
}