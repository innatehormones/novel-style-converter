use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::PromptKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformStatus {
    Pending,
    Running,
    Done,
    Failed,
    /// `Skipped`：on_failure_policy=skip_failed 时失败章保留 error 但跳过；或
    /// 用户在 paused 时显式跳过（resume action=Skip）。
    /// `result_content` 通常为 NULL；`error` 字段保留失败原因。
    Skipped,
    Cancelled,
}

/// 一次转换的结果。挂在 (transformation_novel_id, chapter_id) 上,
/// 每次重跑生成新 row,历史全留(无 UNIQUE 约束)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationChapter {
    pub id: i64,
    pub transformation_novel_id: i64,
    pub chapter_id: i64,
    pub mode: PromptKind,
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
    /// 所属批号;存量散点行为 NULL。
    pub batch_id: Option<i64>,
    /// frontier 章节 id —— 同 tn 内、idx 严格小于本章节、status='done' 的最近一次 tc 行。
    /// 命名沿用 spec(spec §4.1 / §5.8);本片先用 NULL(scheduler 接力时填,Slice 4)。
    pub style_ref_chapter_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewTransformationChapter {
    pub transformation_novel_id: i64,
    pub chapter_id: i64,
    pub mode: PromptKind,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
    pub batch_id: Option<i64>,
    pub style_ref_chapter_id: Option<i64>,
}