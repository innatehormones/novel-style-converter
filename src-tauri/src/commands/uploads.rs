use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::State;

use nsc_core::db::Db;
use nsc_core::encoding::read_text_file;
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
    }
}

/// Read `upload.original_text`; DB field empty (legacy uploads) falls
/// back to reading the raw file from `file_path` and re-decoding.
pub fn read_upload_original_text(u: &Upload) -> Result<String, String> {
    if !u.original_text.is_empty() {
        return Ok(u.original_text.clone());
    }
    read_text_file(Path::new(&u.file_path)).map(|d| d.text)
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

#[tauri::command]
pub fn delete_upload(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let u = db.uploads().get(id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("upload {id} 不存在"))?;
    if let Some(da) = db.data_assets().find_by_upload(id).map_err(|e| e.to_string())? {
        if db.data_assets().is_locked(da.id).map_err(|e| e.to_string())? {
            return Err("upload 对应的 data_asset 已锁定,无法删除".into());
        }
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
        if !u.original_text.is_empty() {
            u.original_text.clone()
        } else {
            read_text_file(Path::new(&u.file_path))?.text
        }
    };
    Ok(Response::new(text.into_bytes()))
}

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
