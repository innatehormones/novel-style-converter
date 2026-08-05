use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::State;

use nsc_core::db::Db;
use nsc_core::db::repo::data_asset::DataAssetWithUpload;
use nsc_core::models::{Chapter, NewDataAsset, NewChapter};
use nsc_core::text::word_count;

/// State 2 章节元数据(无正文,前端按 byte 切片原始 original_text)。
#[derive(Debug, Serialize)]
pub struct DataAssetChapter {
    pub id: i64,
    pub idx: i32,
    pub title: String,
    pub byte_start: i64,
    pub byte_end: i64,
    pub word_count: i32,
}

impl From<&Chapter> for DataAssetChapter {
    fn from(c: &Chapter) -> Self {
        Self {
            id: c.id,
            idx: c.idx,
            title: c.title.clone(),
            byte_start: c.byte_start,
            byte_end: c.byte_end,
            word_count: c.word_count,
        }
    }
}

#[tauri::command]
pub fn list_data_asset_chapters(
    db: State<'_, Arc<Mutex<Db>>>,
    data_asset_id: i64,
) -> Result<Vec<DataAssetChapter>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let chapters = db.chapters().list_by_data_asset(data_asset_id).map_err(|e| e.to_string())?;
    Ok(chapters.iter().map(DataAssetChapter::from).collect())
}

/// 一次性返回 data_asset 对应 upload 的原始原文。
/// 前端按 chapter.byte_start/byte_end 在浏览器侧切片,省去 N 次往返。
#[tauri::command]
pub fn get_data_asset_content(
    db: State<'_, Arc<Mutex<Db>>>,
    data_asset_id: i64,
) -> Result<String, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let da = db.data_assets().get(data_asset_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("data_asset {data_asset_id} 不存在"))?;
    let upload = db.uploads().get(da.upload_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("upload {} 不存在", da.upload_id))?;
    crate::commands::uploads::read_upload_original_text(&upload)
}

/// 把 parse.vue 的章节切片结果落库到新 data_asset。
/// parse.vue 提交入口:为 `upload_id` 创建一个 `data_assets` 行,并把 `chapters`
/// 全部按 byte range 切片入 `chapters` 表。
/// `data_assets.upload_id` 上有 UNIQUE 约束,重复提交会被 SQLite 报 unique
/// violation —— 后端不加业务校验,让错误冒出来。
/// 返回新 `data_asset.id`。
#[tauri::command]
pub fn commit_data_asset(
    db: State<'_, Arc<Mutex<Db>>>,
    upload_id: i64,
    title: String,
    chapters: Vec<crate::commands::chapters::ChapterInput>,
) -> Result<i64, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("标题不能为空".into());
    }
    let db = db.lock().map_err(|e| e.to_string())?;

    let da_id = db.data_assets().insert(&NewDataAsset {
        upload_id,
        title: title.to_string(),
    }).map_err(|e| e.to_string())?;

    let upload = db.uploads().get(upload_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("upload {upload_id} 不存在"))?;
    let text = crate::commands::uploads::read_upload_original_text(&upload)?;

    let new_chapters: Vec<NewChapter> = chapters
        .into_iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let s = c.byte_start.max(0) as usize;
            let e = (c.byte_end.min(text.len() as i64)) as usize;
            if s >= e || e > text.len() {
                return None;
            }
            if !text.is_char_boundary(s) || !text.is_char_boundary(e) {
                return None;
            }
            let body = &text[s..e];
            Some(NewChapter {
                data_asset_id: da_id,
                idx: i as i32,
                title: c.title,
                byte_start: c.byte_start,
                byte_end: c.byte_end,
                word_count: word_count(body) as i32,
            })
        })
        .collect();

    db.chapters().insert_many(da_id, &new_chapters).map_err(|e| e.to_string())?;
    Ok(da_id)
}

/// Library.vue "数据资产" tab:列所有 data_asset + 来源 upload 文件名 + 章节总字数 +
/// 引用此 data_asset 的 transformation_novel 计数。前端按钮 disable 用 `tn_count`。
#[derive(Debug, Serialize)]
pub struct DataAssetRow {
    pub id: i64,
    pub upload_id: i64,
    pub title: String,
    pub parsed_at: String,
    pub filename: String,
    pub byte_size: i64,
    /// SUM(chapters.word_count) WHERE data_asset_id = da.id。
    pub word_count: i64,
    /// COUNT(transformation_novels.id) WHERE data_asset_id = da.id。
    pub tn_count: i64,
}

impl From<DataAssetWithUpload> for DataAssetRow {
    fn from(d: DataAssetWithUpload) -> Self {
        Self {
            id: d.id,
            upload_id: d.upload_id,
            title: d.title,
            parsed_at: d.parsed_at.to_rfc3339(),
            filename: d.filename,
            byte_size: d.byte_size,
            word_count: d.word_count,
            tn_count: d.tn_count,
        }
    }
}

#[tauri::command]
pub fn list_data_assets(
    db: State<'_, Arc<Mutex<Db>>>,
) -> Result<Vec<DataAssetRow>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    Ok(db.data_assets().list_with_upload()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(DataAssetRow::from)
        .collect())
}

/// 旧路由重定向用:upload_id → 对应 data_asset_id(若有)。
/// 路由重定向 helper:`upload_id` → 对应 `data_asset.id`(若有)。前端 router
/// beforeEach 用来把旧的 `/library/:uploadId/...` 路径和新 `/library/data/:id`
/// 路径串起来。返回 `Ok(None)` 表示该 upload 还没解析过。
#[tauri::command]
pub fn find_data_asset_by_upload(
    db: State<'_, Arc<Mutex<Db>>>,
    upload_id: i64,
) -> Result<Option<i64>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    Ok(db.data_assets().find_by_upload(upload_id)
        .map_err(|e| e.to_string())?
        .map(|d| d.id))
}

/// 删除 data_asset。挂着的 transformation_novels / chapters / workflow_results
/// 全部由 FK CASCADE 接走(migration 0005/0006/0012/0013),不需要应用层拦截。
#[tauri::command]
pub fn delete_data_asset(
    db: State<'_, Arc<Mutex<Db>>>,
    data_asset_id: i64,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.data_assets().delete(data_asset_id)
        .map_err(|e| e.to_string())
}