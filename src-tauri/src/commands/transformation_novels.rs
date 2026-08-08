use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;

use nsc_core::db::Db;
use nsc_core::models::{NewTransformationNovel, TransformationNovel};

/// 列表返回的 tn 摘要。
#[derive(Debug, Serialize)]
pub struct TransformationNovelSummary {
    pub id: i64,
    pub data_asset_id: i64,
    pub title: String,
    pub created_at: String,
    pub chapters_count: i64,
    /// 用户填的备注。空串等价于"无备注"。详情页头部标题下面只读展示。
    pub note: String,
}

/// 创建 transformation_novel 的入参。inner DTO 字段保持 snake_case
/// (与 Tauri 的 camelCase outer 自动翻译区分开)。`note` 允许缺省或 `null`
/// —— 都映射为空串,旧调用方/迁移期 payload 兼容。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateTransformationNovelPayload {
    pub data_asset_id: i64,
    pub title: String,
    #[serde(default)]
    pub note: String,
}

/// 更新 transformation_novel 的入参。`note` 允许缺省或 `null`,
/// 都映射为空串(清除已有备注)。目前 UI 没有编辑入口,前端不调用此命令更新 note,
/// 后端仍支持以备未来。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateTransformationNovelPayload {
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub note: String,
}

fn to_summary(db: &Db, n: &TransformationNovel) -> TransformationNovelSummary {
    let chapters_count = db
        .chapters()
        .list_by_data_asset(n.data_asset_id)
        .map(|v| v.len() as i64)
        .unwrap_or(0);
    TransformationNovelSummary {
        id: n.id,
        data_asset_id: n.data_asset_id,
        title: n.title.clone(),
        created_at: n.created_at.to_rfc3339(),
        chapters_count,
        note: n.note.clone(),
    }
}

#[tauri::command]
pub fn list_transformation_novels(
    db: State<'_, Arc<Mutex<Db>>>,
    data_asset_id: Option<i64>,
) -> Result<Vec<TransformationNovelSummary>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let all = match data_asset_id {
        Some(da_id) => db
            .transformation_novels()
            .list_by_data_asset(da_id)
            .map_err(|e| e.to_string())?,
        None => db
            .transformation_novels()
            .list()
            .map_err(|e| e.to_string())?,
    };
    Ok(all.iter().map(|n| to_summary(&db, n)).collect())
}

/// 新建 transformation_novel。先校验 `data_asset_id` 存在 + title 非空;
/// 同 data_asset 允许多本 transformation_novel(每本独立 prompt / model / 上下文)。
/// 返回新 `transformation_novel.id`。
#[tauri::command]
pub fn create_transformation_novel(
    db: State<'_, Arc<Mutex<Db>>>,
    payload: CreateTransformationNovelPayload,
) -> Result<i64, String> {
    let title = payload.title.trim();
    if title.is_empty() {
        return Err("标题不能为空".into());
    }
    let db = db.lock().map_err(|e| e.to_string())?;
    let _da = db
        .data_assets()
        .get(payload.data_asset_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("data_asset {} 不存在", payload.data_asset_id))?;
    db.transformation_novels()
        .insert(&NewTransformationNovel {
            data_asset_id: payload.data_asset_id,
            title: title.to_string(),
            note: payload.note,
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_transformation_novel(
    db: State<'_, Arc<Mutex<Db>>>,
    payload: UpdateTransformationNovelPayload,
) -> Result<(), String> {
    let title = payload.title.trim();
    if title.is_empty() {
        return Err("标题不能为空".into());
    }
    let db = db.lock().map_err(|e| e.to_string())?;
    let cur = db
        .transformation_novels()
        .get(payload.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("transformation_novel {} 不存在", payload.id))?;
    let next = TransformationNovel {
        id: cur.id,
        data_asset_id: cur.data_asset_id,
        title: title.to_string(),
        note: payload.note,
        created_at: cur.created_at,
    };
    db.transformation_novels().update(&next).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_transformation_novel(
    db: State<'_, Arc<Mutex<Db>>>,
    id: i64,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.transformation_novels().delete(id).map_err(|e| e.to_string())
}
