use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;

use nsc_core::db::Db;
use nsc_core::models::{Chapter, NewChapter};
use nsc_core::splitter::{ChapterSplitter, DefaultSplitter, SplitResult};

#[derive(Debug, Serialize)]
pub struct ChapterSegment {
    pub title: String,
    pub byte_start: i64,
    pub byte_end: i64,
    pub word_count: i32,
}

#[derive(Debug, Serialize)]
pub struct ChapterMeta {
    pub id: i64,
    pub idx: i32,
    pub title: String,
    pub word_count: i32,
}

/// 预览专用:从 uploads.original_text 切片,strip 标题行后给前端当正文。
/// 跳过 splitter——预览是 commit 后路径,数据已经在 DB 里。
#[derive(Debug, Serialize)]
pub struct ChapterContentRow {
    pub idx: i32,
    pub title: String,
    pub content: String,
}

/// 切片正文 → 剥首行标题 → trim 边沿空白。
fn extract_body(original: &str, title: &str) -> String {
    let title_trim = title.trim();
    if let Some(nl) = original.find('\n') {
        if original[..nl].trim() == title_trim {
            return original[nl + 1..]
                .trim_start_matches('\n')
                .trim_end()
                .to_string();
        }
    }
    // 首章(head)无标题前缀,或切片单行无换行:整段 trim。
    original.trim().to_string()
}

#[derive(Debug, Deserialize)]
pub struct ChapterInput {
    pub title: String,
    pub byte_start: i64,
    pub byte_end: i64,
}

/// 章节解析页 splitter 入口:对 upload.original_text 做 splitter,
/// 返回原始原文坐标系(byte_start/byte_end 对 raw_text 直接切片用)。
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

    let byte_markers: Vec<usize> = markers
        .unwrap_or_default()
        .into_iter()
        .filter(|m| *m >= 0 && (*m as usize) <= text.len())
        .map(|m| m as usize)
        .collect();
    let byte_suppressed: Vec<usize> = suppressed
        .unwrap_or_default()
        .into_iter()
        .filter(|m| *m >= 0 && (*m as usize) <= text.len())
        .map(|m| m as usize)
        .collect();

    let SplitResult { chapters } =
        DefaultSplitter.split_with_edits(&text, &byte_markers, &byte_suppressed);
    Ok(chapters
        .into_iter()
        .map(|c| ChapterSegment {
            title: c.title,
            byte_start: c.byte_start as i64,
            byte_end: c.byte_end as i64,
            word_count: c.word_count,
        })
        .collect())
}

/// 预览入口:按 data_asset_id 列出已提交章节正文(从 uploads.original_text 切片)。
/// data_asset_id 来自前端 store。
#[tauri::command]
pub fn get_chapter_contents(
    db: State<'_, Arc<Mutex<Db>>>,
    data_asset_id: i64,
) -> Result<Vec<ChapterContentRow>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let da = db.data_assets().get(data_asset_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("data_asset {data_asset_id} 不存在"))?;
    let upload = db.uploads().get(da.upload_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("upload {} 不存在", da.upload_id))?;
    let original = crate::commands::uploads::read_upload_original_text(&upload)?;
    let chapters = db.chapters().list_by_data_asset(data_asset_id).map_err(|e| e.to_string())?;
    Ok(chapters
        .into_iter()
        .map(|c| {
            let s = c.byte_start.max(0) as usize;
            let e = (c.byte_end.max(0) as usize).min(original.len());
            let body = if s < e && e <= original.len() && original.is_char_boundary(s) && original.is_char_boundary(e) {
                &original[s..e]
            } else {
                ""
            };
            ChapterContentRow {
                idx: c.idx,
                title: c.title.clone(),
                content: extract_body(body, &c.title),
            }
        })
        .collect())
}

/// 章节解析页重入用:从 DB 读已提交章节,带 byte 范围。
/// byte_start == NULL 的行(老数据)会被忽略。
#[tauri::command]
pub fn list_committed_segments(
    db: State<'_, Arc<Mutex<Db>>>,
    data_asset_id: i64,
) -> Result<Vec<ChapterSegment>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let rows = db.chapters().list_segments_by_data_asset(data_asset_id).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|r| match (r.byte_start, r.byte_end) {
            (Some(s), Some(e)) => Some(ChapterSegment {
                title: r.title,
                byte_start: s,
                byte_end: e,
                word_count: r.word_count,
            }),
            _ => None,
        })
        .collect())
}

#[tauri::command]
pub fn list_chapters(
    db: State<'_, Arc<Mutex<Db>>>,
    data_asset_id: i64,
) -> Result<Vec<ChapterMeta>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let chapters = db.chapters().list_by_data_asset(data_asset_id).map_err(|e| e.to_string())?;
    Ok(chapters
        .into_iter()
        .map(|c| ChapterMeta {
            id: c.id,
            idx: c.idx,
            title: c.title,
            word_count: c.word_count,
        })
        .collect())
}

#[tauri::command]
pub fn get_chapter(
    db: State<'_, Arc<Mutex<Db>>>,
    chapter_id: i64,
) -> Result<Chapter, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.chapters()
        .get(chapter_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("chapter {chapter_id} 不存在"))
}

/// 章节解析页提交入口:对 upload.original_text 按 byte_start/byte_end 切片,
/// 落库到 data_asset(chapters.data_asset_id 坐标系)。
/// 锁死判定:data_asset.locked_at 非 NULL → 拒绝(对应 data_asset 已锁,不可重解析)。
#[tauri::command]
pub fn parse_chapters(
    db: State<'_, Arc<Mutex<Db>>>,
    data_asset_id: i64,
    segments: Vec<ChapterInput>,
) -> Result<usize, String> {
    let text = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let da = db.data_assets().get(data_asset_id).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("data_asset {data_asset_id} 不存在"))?;
        if db.data_assets().is_locked(data_asset_id).map_err(|e| e.to_string())? {
            return Err("data_asset 已锁定,无法重新解析".into());
        }
        let upload = db.uploads().get(da.upload_id).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("upload {} 不存在", da.upload_id))?;
        crate::commands::uploads::read_upload_original_text(&upload)?
    };

    let mut new_chapters: Vec<NewChapter> = Vec::with_capacity(segments.len());
    for s in segments {
        let s_byte = s.byte_start.max(0) as usize;
        let e_byte = (s.byte_end.min(text.len() as i64)) as usize;
        if s_byte >= e_byte || e_byte > text.len() {
            continue;
        }
        if !text.is_char_boundary(s_byte) || !text.is_char_boundary(e_byte) {
            return Err(format!("byte range {s_byte}..{e_byte} 不在字符边界"));
        }
        let body = &text[s_byte..e_byte];
        let wc = nsc_core::text::word_count(body);
        new_chapters.push(NewChapter {
            data_asset_id,
            idx: 0,
            title: s.title,
            byte_start: s.byte_start,
            byte_end: s.byte_end,
            word_count: wc,
        });
    }

    let db = db.lock().map_err(|e| e.to_string())?;
    let n = db
        .chapters()
        .replace_all_for_data_asset(data_asset_id, &new_chapters)
        .map_err(|e| e.to_string())?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::extract_body;

    #[test]
    fn strips_title_line_for_normal_chapter() {
        let original = "第1章 开始\n\n正文第一段\n正文第二段\n\n";
        assert_eq!(extract_body(original, "第1章 开始"), "正文第一段\n正文第二段");
    }

    #[test]
    fn handles_chapter_with_no_trailing_blank() {
        let original = "第一章\nbody line";
        assert_eq!(extract_body(original, "第一章"), "body line");
    }

    #[test]
    fn falls_back_to_whole_content_when_first_line_mismatches() {
        let original = "序言:没有标题前缀";
        assert_eq!(extract_body(original, "序言"), "序言:没有标题前缀");
    }

    #[test]
    fn handles_single_line_content() {
        let original = "  only line  ";
        assert_eq!(extract_body(original, "only line"), "only line");
    }

    #[test]
    fn title_with_surrounding_whitespace_still_strips() {
        let original = "  第1章 Title  \n\n正文\n";
        assert_eq!(extract_body(original, "第1章 Title"), "正文");
    }
}