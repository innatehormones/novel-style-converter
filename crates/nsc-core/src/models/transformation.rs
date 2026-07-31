use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::PromptKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformMode { Compress, Style }

impl From<PromptKind> for TransformMode {
    fn from(k: PromptKind) -> Self {
        match k {
            PromptKind::Compress => TransformMode::Compress,
            PromptKind::Style => TransformMode::Style,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformStatus { Pending, Running, Done, Failed, Cancelled }

/// 一次转换的结果。挂在 (transformation_novel_id, chapter_id) 上,
/// 每次重跑生成新 row,历史全留(无 UNIQUE 约束)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationChapter {
    pub id: i64,
    pub transformation_novel_id: i64,
    pub chapter_id: i64,
    pub mode: TransformMode,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
    pub status: TransformStatus,
    pub result_content: Option<String>,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewTransformationChapter {
    pub transformation_novel_id: i64,
    pub chapter_id: i64,
    pub mode: TransformMode,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
}