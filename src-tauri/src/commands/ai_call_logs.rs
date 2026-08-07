use std::sync::{Arc, Mutex};

use nsc_core::db::Db;
use nsc_core::models::{AiCallBusiness, AiCallLogFilter, AiCallStatus};
use serde::Deserialize;
use tauri::State;

/// 列表过滤 DTO —— 前端发 snake_case,内层字段保持 Rust 原名。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AiCallLogFilterDto {
    pub business: Option<String>,
    pub model_config_id: Option<i64>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

impl AiCallLogFilterDto {
    fn into_filter(self) -> Result<AiCallLogFilter, String> {
        let business = match self.business.as_deref() {
            None => None,
            Some("transform_chapter") => Some(AiCallBusiness::TransformChapter),
            Some("test_model") => Some(AiCallBusiness::TestModel),
            Some(other) => return Err(format!("unknown business: {other}")),
        };
        let status = match self.status.as_deref() {
            None => None,
            Some("success") => Some(AiCallStatus::Success),
            Some("failed") => Some(AiCallStatus::Failed),
            Some(other) => return Err(format!("unknown status: {other}")),
        };
        Ok(AiCallLogFilter { business, model_config_id: self.model_config_id, status, limit: self.limit })
    }
}

/// 列表:按 filter 过滤,时间倒序。
#[tauri::command]
pub fn list_ai_call_logs(
    db: State<'_, Arc<Mutex<Db>>>,
    filter: AiCallLogFilterDto,
) -> Result<Vec<nsc_core::models::AiCallLog>, String> {
    let f = filter.into_filter()?;
    let db = db.lock().map_err(|e| e.to_string())?;
    db.ai_call_logs().list(&f).map_err(|e| e.to_string())
}

/// 单行详情:UI 详情页拉。
#[tauri::command]
pub fn get_ai_call_log(
    db: State<'_, Arc<Mutex<Db>>>,
    id: i64,
) -> Result<Option<nsc_core::models::AiCallLog>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.ai_call_logs().get(id).map_err(|e| e.to_string())
}

/// 清空全部日志 —— UI 清空按钮专用,返回删除行数供 toast。
#[tauri::command]
pub fn clear_ai_call_logs(
    db: State<'_, Arc<Mutex<Db>>>,
) -> Result<usize, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.ai_call_logs().clear().map_err(|e| e.to_string())
}

/// 软引用反查:从 transformation_chapter 找历史 AI 调用
/// —— transformation_chapter 删了也不影响日志可见。
#[tauri::command]
pub fn list_ai_call_logs_by_context(
    db: State<'_, Arc<Mutex<Db>>>,
    context_type: String,
    context_id: i64,
) -> Result<Vec<nsc_core::models::AiCallLog>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.ai_call_logs()
        .list_by_context(&context_type, context_id)
        .map_err(|e| e.to_string())
}