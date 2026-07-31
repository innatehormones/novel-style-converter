use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::ipc::Response;
use tauri::State;

use nsc_core::db::Db;
use nsc_core::encoding::{decode_to_utf8, read_text_file};
use nsc_core::models::{NewUpload, Upload};

/// Upload listing IPC DTO。只承载 upload 自身字段;data_asset 相关信息
/// 属于后续 Task 7 的 DataAssetSummary,本任务不混合到上传响应里。
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
    pub filename: String,
    pub bytes: Vec<u8>,
}

fn uploads_dir() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("novel-style-converter").join("uploads")
}

fn ensure_dir(p: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(p)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    out.iter().map(|b| format!("{:02x}", b)).collect()
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

/// 读 upload.original_text;DB 字段为空时(老版本上传的)兜底从 file_path 读 raw 文件再 decode。
/// 老 DB 行可能没 original_text,但 raw 文件在 disk 上还在;统一走这里防止 read 路径拿到 ""。
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

#[tauri::command]
pub fn upload_file(
    db: State<'_, Arc<Mutex<Db>>>,
    payload: UploadFilePayload,
) -> Result<UploadSummary, String> {
    if payload.filename.trim().is_empty() {
        return Err("文件名不能为空".into());
    }
    if payload.bytes.is_empty() {
        return Err("文件为空".into());
    }
    // 早失败:编码解析失败 = 上传失败,不入库。
    let decoded = decode_to_utf8(&payload.bytes)?;
    let text = decoded.text;

    let sha = sha256_hex(&payload.bytes);

    let db = db.lock().map_err(|e| e.to_string())?;
    let existing = db
        .uploads()
        .find_by_sha256(&sha)
        .map_err(|e| e.to_string())?;
    if let Some(id) = existing {
        let u = db.uploads().get(id).map_err(|e| e.to_string())?
            .ok_or_else(|| "upload row missing".to_string())?;
        return Ok(to_summary(&u));
    }

    let dir = uploads_dir();
    ensure_dir(&dir).map_err(|e| format!("创建目录失败: {e}"))?;
    let file_path = dir.join(format!("{sha}.txt"));
    std::fs::write(&file_path, &payload.bytes).map_err(|e| format!("写文件失败: {e}"))?;

    let id = db.uploads().insert(&NewUpload {
        sha256: sha,
        filename: payload.filename.trim().to_string(),
        byte_size: payload.bytes.len() as i64,
        file_path: file_path.to_string_lossy().to_string(),
        original_text: text,
    }).map_err(|e| e.to_string())?;

    let u = db.uploads().get(id).map_err(|e| e.to_string())?
        .ok_or_else(|| "upload row missing".to_string())?;
    Ok(to_summary(&u))
}

#[tauri::command]
pub fn delete_upload(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let u = db.uploads().get(id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("upload {id} 不存在"))?;
    // data_asset 锁死 → 不允许删 upload(锁死语义由 Task 4 完整给出,这里已对齐)。
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
            // 兜底:旧库没 original_text,从 file_path 读。
            read_text_file(Path::new(&u.file_path))?.text
        }
    };
    Ok(Response::new(text.into_bytes()))
}

/// State 1 编辑入口:把 textarea 里的全文写回 uploads.original_text。
/// 已有 data_asset 关联时拒绝(unlocked 也拒)——raw_text 改了 chapters.byte_range
/// 就指向无效偏移,前端没法自动重切,必须先 delete_data_asset 再重解析。
/// 这是真相源头,前端 hasDataAsset disable 只是 UX,后端必须独立校验防绕过。
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