use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Terminated,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnFailurePolicy {
    /// 章节失败 → batch 转 Paused 等用户决策
    PauseAndReview,
    /// 章节失败 → 同 batch 后续章节 cancelled + batch 转 Terminated
    Terminate,
    /// 章节失败 → 该章标 Skipped,继续 dispatch 下一章(batch 留 Running)
    SkipFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Batch {
    pub id: i64,
    pub transformation_novel_id: i64,
    pub label: Option<String>,
    pub on_failure_policy: OnFailurePolicy,
    pub status: BatchStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewBatch {
    pub transformation_novel_id: i64,
    pub label: Option<String>,
    pub on_failure_policy: OnFailurePolicy,
}

/// scheduler / IPC 共用的用户决策动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeAction {
    /// 重试该章
    Retry(i64),
    /// 标 skipped,继续走完本 batch
    Skip(i64),
    /// 终止整批
    Terminate,
}
