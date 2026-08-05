use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::State;

use nsc_core::db::Db;
use nsc_core::models::Upload;
use nsc_core::upload;

/// Upload listing IPC DTO. Only carries upload self-fields; data_asset
/// related info belongs to DataAssetSummary (Task 7).
#[derive(Debug, Serialize)]
pub struct UploadSummary {
    pub id: i64,
    pub sha256: String,
    pub filename: String,
    pub byte_size: i64,
    pub uploaded_at: String,
    pub file_path: String,
    /// 字数(zh-aware,汉字 + 字母 + 数字)。upload_file() 时由 nsc_core::text::word_count 一次算好,
    /// list 列表无需重扫原文。
    pub word_count: i64,
}

#[derive(Debug, Deserialize)]
pub struct UploadFilePayload {
    pub file_path: String,
    pub filename: String,
}

fn uploads_dir() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("novel-style-converter").join("uploads")
}

fn to_summary(u: &Upload) -> UploadSummary {
    UploadSummary {
        id: u.id,
        sha256: u.sha256.clone(),
        filename: u.filename.clone(),
        byte_size: u.byte_size,
        uploaded_at: u.uploaded_at.to_rfc3339(),
        file_path: u.file_path.clone(),
        word_count: u.word_count,
    }
}

/// Read `upload.original_text`. Empty field is a data integrity error
/// (new uploads always populate it; if it's empty, something wiped it).
pub fn read_upload_original_text(u: &Upload) -> Result<String, String> {
    if u.original_text.is_empty() {
        return Err(format!(
            "upload {} ({}) 的 original_text 为空,文件路径 {}。请重新上传该文件。",
            u.id, u.filename, u.file_path
        ));
    }
    Ok(u.original_text.clone())
}

#[tauri::command]
pub fn list_uploads(db: State<'_, Arc<Mutex<Db>>>) -> Result<Vec<UploadSummary>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let ups = db.uploads().list().map_err(|e| e.to_string())?;
    Ok(ups.iter().map(to_summary).collect())
}

/// Register a new upload from a user-chosen file path. Delegates to
/// `nsc_core::upload::upload_file` for all business logic (decode, hash,
/// dedup, atomic write, DB insert with rollback).
#[tauri::command]
pub fn upload_file(
    db: State<'_, Arc<Mutex<Db>>>,
    payload: UploadFilePayload,
) -> Result<UploadSummary, String> {
    let dir = uploads_dir();
    let source = PathBuf::from(&payload.file_path);
    let db_guard = db.lock().map_err(|e| e.to_string())?;
    let u = upload::upload_file(&db_guard, &source, &payload.filename, &dir)
        .map_err(|e| e.to_string())?;
    Ok(to_summary(&u))
}

/// 删除 upload。data_asset 也通过 FK CASCADE 一起清掉,业务层不需要拦。
/// 删 upload。如果有关联 data_asset,FK CASCADE 会把它一起带走(数据资产页看不到了),
/// 所以必须让用户先去数据资产页删。计划文档 2026-07-31-upload-refactor.md 明确这条 guard。
#[tauri::command]
pub fn delete_upload(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let u = db.uploads().get(id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("upload {id} 不存在"))?;
    if db.data_assets().find_by_upload(id).map_err(|e| e.to_string())?.is_some() {
        return Err("该 upload 有关联的数据资产,请先在数据资产页删除".into());
    }
    let _ = std::fs::remove_file(&u.file_path);
    db.uploads().delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_upload(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<UploadSummary, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let u = db.uploads().get(id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("upload {id} 不存在"))?;
    Ok(to_summary(&u))
}

#[tauri::command]
pub fn get_upload_text(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<Response, String> {
    let text = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let u = db.uploads().get(id).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("upload {id} 不存在"))?;
        if u.original_text.is_empty() {
            return Err(format!(
                "upload {} 的 original_text 为空。请重新上传该文件。",
                u.id
            ));
        }
        u.original_text.clone()
    };
    Ok(Response::new(text.into_bytes()))
}

/// 修改 upload.original_text。改完会让已存在的 chapter 切片坐标系失效,
/// 所以如果该 upload 已有 data_asset,拒绝(让用户先在数据资产页删除)。
/// 计划文档 2026-07-31-upload-refactor.md 明确这条 guard。
#[tauri::command]
pub fn update_upload_text(
    db: State<'_, Arc<Mutex<Db>>>,
    id: i64,
    text: String,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    if db.uploads().get(id).map_err(|e| e.to_string())?.is_none() {
        return Err(format!("upload {id} 不存在"));
    }
    if db.data_assets().find_by_upload(id).map_err(|e| e.to_string())?.is_some() {
        return Err("该 upload 已有 data_asset,无法修改原文。请先在数据资产页删除后再修改。".into());
    }
    db.uploads().set_original_text(id, &text).map_err(|e| e.to_string())
}
