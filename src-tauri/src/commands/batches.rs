use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;

use nsc_core::db::repo::BatchStatusCount;
use nsc_core::db::Db;
use nsc_core::error::Error;
use nsc_core::models::{Batch, BatchStatus, NewBatch, OnFailurePolicy, ResumeAction, TransformMode};
use nsc_core::transformer::{BatchOverrides, BatchScheduler};

#[derive(Debug, Deserialize)]
pub struct CreateBatchPayload {
    pub tn_id: i64,
    pub label: Option<String>,
    /// 'pause_and_review' | 'terminate' | 'skip_failed'
    pub on_failure_policy: String,
    /// Slice 4 scheduler 接管前不入队,字段先保留供前端透传。
    #[serde(default)]
    pub chapter_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBatchPayload {
    pub label: Option<String>,
    pub on_failure_policy: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BatchSummary {
    pub id: i64,
    pub tn_id: i64,
    pub label: Option<String>,
    pub on_failure_policy: OnFailurePolicy,
    pub status: BatchStatus,
    pub created_at: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
}

fn parse_policy(s: &str) -> Result<OnFailurePolicy, Error> {
    match s {
        "pause_and_review" => Ok(OnFailurePolicy::PauseAndReview),
        "terminate"        => Ok(OnFailurePolicy::Terminate),
        "skip_failed"      => Ok(OnFailurePolicy::SkipFailed),
        other => Err(Error::Validation(format!("未知的 on_failure_policy: {other}"))),
    }
}

fn to_summary(b: &Batch) -> BatchSummary {
    BatchSummary {
        id: b.id,
        tn_id: b.transformation_novel_id,
        label: b.label.clone(),
        on_failure_policy: b.on_failure_policy,
        status: b.status,
        created_at: b.created_at.to_rfc3339(),
        started_at: b.started_at.map(|t| t.to_rfc3339()),
        ended_at: b.ended_at.map(|t| t.to_rfc3339()),
    }
}

#[tauri::command]
pub fn list_batches(
    db: State<'_, Arc<Mutex<Db>>>,
    tn_id: i64,
) -> Result<Vec<BatchSummary>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.batches()
        .list_by_tn(tn_id)
        .map_err(|e| e.to_string())
        .map(|v| v.iter().map(to_summary).collect())
}

#[tauri::command]
pub fn get_batch(
    db: State<'_, Arc<Mutex<Db>>>,
    batch_id: i64,
) -> Result<BatchSummary, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let b = db.batches().get(batch_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("batch {batch_id} 不存在"))?;
    Ok(to_summary(&b))
}

#[tauri::command]
pub fn create_batch(
    db: State<'_, Arc<Mutex<Db>>>,
    payload: CreateBatchPayload,
) -> Result<i64, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let policy = parse_policy(&payload.on_failure_policy).map_err(|e| e.to_string())?;
    if db.transformation_novels().get(payload.tn_id).map_err(|e| e.to_string())?.is_none() {
        return Err(format!("transformation_novel {} 不存在", payload.tn_id));
    }
    let id = db.batches().insert(&NewBatch {
        transformation_novel_id: payload.tn_id,
        label: payload.label,
        on_failure_policy: policy,
    }).map_err(|e| e.to_string())?;
    // chapter_ids 在 Slice 4 由 BatchScheduler 入队,本命令只创建 batch 行。
    let _ = payload.chapter_ids;
    Ok(id)
}

#[tauri::command]
pub fn update_batch(
    db: State<'_, Arc<Mutex<Db>>>,
    batch_id: i64,
    payload: UpdateBatchPayload,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cur = db.batches().get(batch_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("batch {batch_id} 不存在"))?;
    if matches!(cur.status, BatchStatus::Running) {
        return Err("batch 正在运行,不可改 label / on_failure_policy".into());
    }
    let new_label = payload.label.or(cur.label);
    let new_policy = match payload.on_failure_policy.as_deref() {
        None => cur.on_failure_policy,
        Some(s) => parse_policy(s).map_err(|e| e.to_string())?,
    };
    let next = Batch {
        id: cur.id,
        transformation_novel_id: cur.transformation_novel_id,
        label: new_label,
        on_failure_policy: new_policy,
        status: cur.status,
        created_at: cur.created_at,
        started_at: cur.started_at,
        ended_at: cur.ended_at,
    };
    db.batches().update(&next).map_err(|e| e.to_string())
}

/// 列出 batch 内所有 tc 行 + join chapter 标题/idx。
#[tauri::command]
pub fn list_batch_chapters(
    db: State<'_, Arc<Mutex<Db>>>,
    batch_id: i64,
) -> Result<Vec<super::transformations::TransformationChapterRow>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let rows = db.transformation_chapters()
        .list_by_batch(batch_id)
        .map_err(|e| e.to_string())?;
    let batch = db.batches().get(batch_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("batch {batch_id} 不存在"))?;
    let tn = db.transformation_novels().get(batch.transformation_novel_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("tn {} 不存在", batch.transformation_novel_id))?;
    Ok(super::transformations::join_chapter_info(&db, tn.data_asset_id, rows))
}

/// 派发已存在的 Pending batch：自动取 TN 全量章节,落 tc 行,派首章。
/// 内部委派给 `BatchScheduler::dispatch_batch`。
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct DispatchOverrides {
    #[serde(default)] pub prompt_id: Option<i64>,
    #[serde(default)] pub model_config_id: Option<i64>,
    #[serde(default)] pub mode: Option<String>,
    #[serde(default)] pub ctx_prev_original: Option<i32>,
    #[serde(default)] pub ctx_prev_transformed: Option<i32>,
    #[serde(default)] pub ctx_next_original: Option<i32>,
}

impl DispatchOverrides {
    fn into_core(self) -> Result<BatchOverrides, Error> {
        let mode = match self.mode.as_deref() {
            None => None,
            Some("compress") => Some(TransformMode::Compress),
            Some("style") => Some(TransformMode::Style),
            Some(other) => return Err(Error::Validation(format!("未知 mode: {other}"))),
        };
        Ok(BatchOverrides {
            prompt_id: self.prompt_id,
            model_config_id: self.model_config_id,
            mode,
            ctx_prev_original: self.ctx_prev_original,
            ctx_prev_transformed: self.ctx_prev_transformed,
            ctx_next_original: self.ctx_next_original,
        })
    }
}

#[tauri::command]
pub async fn dispatch_batch(
    batch_id: i64,
    overrides: DispatchOverrides,
    scheduler: tauri::State<'_, Arc<BatchScheduler>>,
) -> Result<BatchSummary, String> {
    let scheduler = scheduler.inner().clone();
    let overrides = overrides.into_core().map_err(|e| e.to_string())?;
    let res = tokio::task::spawn_blocking(move || scheduler.dispatch_batch(batch_id, overrides))
        .await
        .map_err(|e| format!("dispatch_batch join error: {e}"))?
        .map_err(|e| e.to_string())?;
    Ok(to_summary(&res))
}

#[tauri::command]
pub fn count_batches_by_status(
    db: State<'_, Arc<Mutex<Db>>>,
    tn_id: i64,
) -> Result<BatchStatusCount, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.batches().count_by_status(tn_id).map_err(|e| e.to_string())
}

/// `resume_batch` 入参。`kind` 决定动作；`chapter_id` 仅 retry/skip 时必填。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResumeActionPayload {
    pub kind: String,
    #[serde(default)]
    pub chapter_id: Option<i64>,
}

impl ResumeActionPayload {
    fn into_core(self) -> ResumeAction {
        match self.kind.as_str() {
            "retry" => ResumeAction::Retry(
                self.chapter_id.expect("retry 必须带 chapter_id"),
            ),
            "skip" => ResumeAction::Skip(
                self.chapter_id.expect("skip 必须带 chapter_id"),
            ),
            "terminate" => ResumeAction::Terminate,
            other => panic!("未知 ResumeAction kind: {other}"),
        }
    }
}

#[tauri::command]
pub async fn resume_batch(
    batch_id: i64,
    action: ResumeActionPayload,
    scheduler: tauri::State<'_, Arc<BatchScheduler>>,
) -> Result<Batch, String> {
    let scheduler = scheduler.inner().clone();
    let resume_action = action.into_core();
    let res = tokio::task::spawn_blocking(move || scheduler.resume(batch_id, resume_action))
        .await
        .map_err(|e| format!("resume_batch join error: {e}"))?
        .map_err(|e| e.to_string())?;
    Ok(res)
}

#[cfg(test)]
mod tests {
    //! T9 阶段:验证 IPC payload 的 serde 形状 + `parse_policy` 边界。
    use super::{parse_policy, CreateBatchPayload, UpdateBatchPayload};
    use nsc_core::models::{BatchStatus, OnFailurePolicy};
    use serde_json::json;

    #[test]
    fn create_payload_deserializes_snake_case() {
        let raw = json!({
            "tn_id": 7,
            "label": "batch-A",
            "on_failure_policy": "terminate",
            "chapter_ids": [1, 2, 3],
        });
        let p: CreateBatchPayload = serde_json::from_value(raw).expect("serde");
        assert_eq!(p.tn_id, 7);
        assert_eq!(p.label.as_deref(), Some("batch-A"));
        assert_eq!(p.on_failure_policy, "terminate");
        assert_eq!(p.chapter_ids, vec![1, 2, 3]);
    }

    #[test]
    fn create_payload_label_optional_chapter_ids_default_empty() {
        let raw = json!({ "tn_id": 1, "on_failure_policy": "pause_and_review" });
        let p: CreateBatchPayload = serde_json::from_value(raw).expect("serde");
        assert!(p.label.is_none());
        assert!(p.chapter_ids.is_empty());
    }

    #[test]
    fn update_payload_all_fields_optional() {
        let raw = json!({});
        let p: UpdateBatchPayload = serde_json::from_value(raw).expect("serde");
        assert!(p.label.is_none());
        assert!(p.on_failure_policy.is_none());
    }

    #[test]
    fn parse_policy_accepts_three_variants() {
        assert!(matches!(parse_policy("pause_and_review"), Ok(OnFailurePolicy::PauseAndReview)));
        assert!(matches!(parse_policy("terminate"),        Ok(OnFailurePolicy::Terminate)));
        assert!(matches!(parse_policy("skip_failed"),      Ok(OnFailurePolicy::SkipFailed)));
    }

    #[test]
    fn parse_policy_rejects_unknown_with_validation_error() {
        let err = parse_policy("explode").unwrap_err();
        // Error::Validation 的 message 含 "未知的 on_failure_policy"
        assert!(format!("{err}").contains("未知的 on_failure_policy"), "got: {err}");
    }

    #[test]
    fn summary_serializes_default_mode_and_status_in_snake_case() {
        // BatchSummary 是 IPC 响应 —— 验证 enum 走 snake_case。
        let s = super::BatchSummary {
            id: 1,
            tn_id: 2,
            label: None,
            on_failure_policy: OnFailurePolicy::SkipFailed,
            status: BatchStatus::Paused,
            created_at: "1970-01-01T00:00:00Z".into(),
            started_at: None,
            ended_at: None,
        };
        let v: serde_json::Value = serde_json::to_value(&s).expect("serialize");
        assert_eq!(v["on_failure_policy"], serde_json::json!("skip_failed"));
        assert_eq!(v["status"], serde_json::json!("paused"));
    }
}
