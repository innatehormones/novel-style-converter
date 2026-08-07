use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;

use nsc_core::db::Db;
use nsc_core::models::{Chapter, NewChapter};
use nsc_core::splitter::{ChapterSplitter, DefaultSplitter, SplitResult};

/// parse 阶段返回的章节预览：title + body（不再带 byte 偏移）。
/// 用户在 parse.vue 编辑标题 / 合并 / 加 marker 重切时，后端直接返回完整正文。
#[derive(Debug, Serialize)]
pub struct ChapterSegment {
    pub title: String,
    pub content: String,
    pub word_count: i32,
}

#[derive(Debug, Serialize)]
pub struct ChapterMeta {
    pub id: i64,
    pub idx: i32,
    pub title: String,
    pub word_count: i32,
    pub body: String,
}

#[derive(Debug, Serialize)]
pub struct ChapterContentRow {
    pub idx: i32,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ChapterInput {
    pub title: String,
    pub content: String,
}

/// parse wizard 入口：对 upload.original_text 跑 splitter，应用 markers（byte 偏移）
/// + suppressed（章节 idx），返回 ChapterSegment 列表直接展示。
#[tauri::command]
pub fn list_chapter_segments(
    db: State<'_, Arc<Mutex<Db>>>,
    upload_id: i64,
    markers: Option<Vec<i64>>,
    suppressed: Option<Vec<i64>>,
) -> Result<Vec<ChapterSegment>, String> {
    let text = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let u = db.uploads().get(upload_id).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("upload {upload_id} 不存在"))?;
        crate::commands::uploads::read_upload_original_text(&u)?
    };

    let byte_markers: Vec<usize> = markers.unwrap_or_default()
        .into_iter()
        .filter(|m| *m >= 0 && (*m as usize) <= text.len())
        .map(|m| m as usize)
        .collect();
    // suppressed 在新模型下不再有意义（chapter 无 byte_start）—— 保留参数兼容旧前端，忽略
    let _ = suppressed;

    let SplitResult { chapters } =
        DefaultSplitter.split_with_edits(&text, &byte_markers, &[]);
    Ok(chapters
        .into_iter()
        .map(|c| ChapterSegment {
            title: c.title,
            content: c.content,
            word_count: c.word_count,
        })
        .collect())
}

/// 已提交章节列表（data_asset 视角）。
#[tauri::command]
pub fn get_chapter_contents(
    db: State<'_, Arc<Mutex<Db>>>,
    data_asset_id: i64,
) -> Result<Vec<ChapterContentRow>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let chapters = db.chapters().list_by_data_asset(data_asset_id).map_err(|e| e.to_string())?;
    Ok(chapters.into_iter().map(|c| ChapterContentRow {
        idx: c.idx,
        title: c.title.clone(),
        content: c.body,
    }).collect())
}

/// 老接口：返回已提交章节的 title + word_count + id（不带 byte）。
/// 兼容老 parse.vue / dataAsset store 的旧字段。
#[tauri::command]
pub fn list_committed_segments(
    db: State<'_, Arc<Mutex<Db>>>,
    data_asset_id: i64,
) -> Result<Vec<ChapterSegment>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let chapters = db.chapters().list_by_data_asset(data_asset_id).map_err(|e| e.to_string())?;
    Ok(chapters.into_iter().map(|c| ChapterSegment {
        title: c.title,
        content: c.body,
        word_count: c.word_count,
    }).collect())
}

/// 列出 data_asset 下全部章节（含 body），给前端做完整视图。
#[tauri::command]
pub fn list_chapters(
    db: State<'_, Arc<Mutex<Db>>>,
    data_asset_id: i64,
) -> Result<Vec<ChapterMeta>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let chapters = db.chapters().list_by_data_asset(data_asset_id).map_err(|e| e.to_string())?;
    Ok(chapters.into_iter().map(|c| ChapterMeta {
        id: c.id,
        idx: c.idx,
        title: c.title.clone(),
        word_count: c.word_count,
        body: c.body,
    }).collect())
}

#[tauri::command]
pub fn get_chapter(
    db: State<'_, Arc<Mutex<Db>>>,
    chapter_id: i64,
) -> Result<Chapter, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.chapters().get(chapter_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("chapter {chapter_id} 不存在"))
}

/// commit 时用：parse.vue 把编辑好的 segments 直接提交，每段是 title + content。
#[tauri::command]
pub fn parse_chapters(
    db: State<'_, Arc<Mutex<Db>>>,
    data_asset_id: i64,
    segments: Vec<ChapterInput>,
) -> Result<usize, String> {
    let new_chapters: Vec<NewChapter> = segments.into_iter().map(|s| {
        let wc = nsc_core::text::word_count(&s.content);
        NewChapter {
            data_asset_id,
            idx: 0,
            title: s.title,
            body: s.content,
            word_count: wc,
        }
    }).collect();
    let db = db.lock().map_err(|e| e.to_string())?;
    let n = db.chapters().replace_all_for_data_asset(data_asset_id, &new_chapters)
        .map_err(|e| e.to_string())?;
    Ok(n)
}
