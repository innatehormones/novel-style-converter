use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: i64,
    pub data_asset_id: i64,
    pub idx: i32,
    pub title: String,
    pub body: String,
    pub word_count: i32,
}

#[derive(Debug, Clone)]
pub struct NewChapter {
    pub data_asset_id: i64,
    pub idx: i32,
    pub title: String,
    pub body: String,
    pub word_count: i32,
}

impl Default for NewChapter {
    fn default() -> Self {
        Self { data_asset_id: 0, idx: 0, title: String::new(), body: String::new(), word_count: 0 }
    }
}
