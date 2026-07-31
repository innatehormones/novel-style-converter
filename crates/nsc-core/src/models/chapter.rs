use serde::{Deserialize, Serialize};

/// 章节切片。byte_start / byte_end 在 uploads.original_text 坐标系(详见 data_asset.rs 顶部 doc)。
/// 一旦 data_asset 锁(locked_at 非 NULL),本表 immutable。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: i64,
    pub data_asset_id: i64,
    pub idx: i32,
    pub title: String,
    pub byte_start: i64,
    pub byte_end: i64,
    pub word_count: i32,
}

#[derive(Debug, Clone)]
pub struct NewChapter {
    pub data_asset_id: i64,
    pub idx: i32,
    pub title: String,
    pub byte_start: i64,
    pub byte_end: i64,
    pub word_count: i32,
}

impl Default for NewChapter {
    fn default() -> Self {
        Self {
            data_asset_id: 0,
            idx: 0,
            title: String::new(),
            byte_start: 0,
            byte_end: 0,
            word_count: 0,
        }
    }
}