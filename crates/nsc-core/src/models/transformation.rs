use std::str::FromStr;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewStatus {
    Generating,
    Done,
    Failed,
}

fn parse_status(s: &str) -> Option<PreviewStatus> {
    match s {
        "generating" => Some(PreviewStatus::Generating),
        "done" => Some(PreviewStatus::Done),
        "failed" => Some(PreviewStatus::Failed),
        _ => None,
    }
}

impl FromStr for PreviewStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_status(s).ok_or_else(|| format!("unknown preview status: {}", s))
    }
}

/// 一次单章节预览尝试 —— 用户可以为一个 (batch_id, chapter_id) 多次生成,对比 / 编辑 / 提交。
/// 提交(transformation_chapters.content 被覆写)后由 repo.delete_by_chapter 清空全部预览行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterPreviewRow {
    pub id: i64,
    pub batch_id: i64,
    pub chapter_id: i64,
    pub custom_input: Option<String>,
    pub preview_content: Option<String>,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
    pub error: Option<String>,
    pub status: PreviewStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 用户在「新建工作流」试运行区满意的首章结果,作为创建工作流时的 seed。
/// 后端事务内把 idx 最小那个 chapter 对应的 tc 标 done + 写 result_content。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewFirstChapter {
    pub content: String,
    pub tokens_in: i32,
    pub tokens_out: i32,
}
