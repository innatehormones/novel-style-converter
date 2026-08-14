use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;

use nsc_core::db::Db;
use nsc_core::error::Error;
use nsc_core::models::{Batch, BatchStatus, OnFailurePolicy, PromptKind, TransformStatus};
use nsc_core::transformer::{BatchScheduler, WorkflowCreate};

const CONTENT_PREVIEW_CHARS: usize = 80;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateWorkflowPayload {
    pub tn_id: i64,
    pub label: Option<String>,
    pub chapter_ids: Vec<i64>,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub mode: String,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
    pub on_failure_policy: String,
}

impl CreateWorkflowPayload {
    fn into_core(self) -> Result<WorkflowCreate, Error> {
        let mode = match self.mode.as_str() {
            "compress" => PromptKind::Compress,
            "style" => PromptKind::Style,
            other => return Err(Error::Validation(format!("未知 mode: {other}"))),
        };
        let on_failure_policy = match self.on_failure_policy.as_str() {
            "pause_and_review" => OnFailurePolicy::PauseAndReview,
            "terminate" => OnFailurePolicy::Terminate,
            "skip_failed" => OnFailurePolicy::SkipFailed,
            other => return Err(Error::Validation(format!("未知 on_failure_policy: {other}"))),
        };
        Ok(WorkflowCreate {
            transformation_novel_id: self.tn_id,
            label: self.label,
            chapter_ids: self.chapter_ids,
            prompt_id: self.prompt_id,
            model_config_id: self.model_config_id,
            mode,
            ctx_prev_original: self.ctx_prev_original,
            ctx_prev_transformed: self.ctx_prev_transformed,
            ctx_next_original: self.ctx_next_original,
            on_failure_policy,
        })
    }
}

/// 单章节预览提交入参(spec §4.2) —— 必传 batch_id / chapter_id / draft_content;可选 source_preview_id 用于透传 tokens。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CommitPreviewInput {
    pub batch_id: i64,
    pub chapter_id: i64,
    pub draft_content: String,
    pub source_preview_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowSummary {
    pub id: i64,
    pub tn_id: i64,
    pub label: Option<String>,
    pub status: BatchStatus,
    pub created_at: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub done_count: i64,
    pub failed_count: i64,
    pub skipped_count: i64,
    pub total_count: i64,
    pub promoted_count: i64,
}

#[derive(Debug, Serialize)]
pub struct WorkflowChapterRow {
    pub tc_id: i64,
    pub chapter_id: i64,
    pub chapter_idx: i32,
    pub chapter_title: String,
    pub status: TransformStatus,
    pub error: Option<String>,
    pub content_preview: Option<String>,
    pub is_empty_slot: bool,
}

#[derive(Debug, Serialize)]
pub struct SourceChapterRow {
    pub chapter_id: i64,
    pub idx: i32,
    pub title: String,
    pub word_count: i32,
    pub non_empty_result_count: i64,
}

#[derive(Debug, Serialize)]
pub struct ChapterWorkflowResultRow {
    pub batch_id: i64,
    pub batch_label: Option<String>,
    pub batch_status: BatchStatus,
    pub batch_ended_at: Option<String>,
    pub content: Option<String>,
    pub status: TransformStatus,
}

fn parse_batch_status(s: &str) -> Result<BatchStatus, String> {
    match s {
        "pending" => Ok(BatchStatus::Pending),
        "running" => Ok(BatchStatus::Running),
        "stopped" => Ok(BatchStatus::Stopped),
        "paused" => Ok(BatchStatus::Paused),
        "completed" => Ok(BatchStatus::Completed),
        "terminated" => Ok(BatchStatus::Terminated),
        "cancelled" => Ok(BatchStatus::Cancelled),
        other => Err(format!("unknown batch status: {other}")),
    }
}

fn parse_transform_status(s: &str) -> Result<TransformStatus, String> {
    match s {
        "pending" => Ok(TransformStatus::Pending),
        "running" => Ok(TransformStatus::Running),
        "done" => Ok(TransformStatus::Done),
        "failed" => Ok(TransformStatus::Failed),
        "skipped" => Ok(TransformStatus::Skipped),
        "cancelled" => Ok(TransformStatus::Cancelled),
        other => Err(format!("unknown transform status: {other}")),
    }
}

fn to_summary(db: &Db, b: &Batch) -> WorkflowSummary {
    let (done, failed, skipped, total): (i64, i64, i64, i64) = db.conn.query_row(
        "SELECT \
            COALESCE(SUM(CASE WHEN status='done' THEN 1 ELSE 0 END), 0), \
            COALESCE(SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END), 0), \
            COALESCE(SUM(CASE WHEN status='skipped' THEN 1 ELSE 0 END), 0), \
            COUNT(*) \
         FROM transformation_chapters WHERE batch_id = ?1",
        rusqlite::params![b.id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).unwrap_or((0, 0, 0, 0));
    let promoted_count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM data_assets WHERE source_workflow_id = ?1",
        rusqlite::params![b.id],
        |row| row.get(0),
    ).unwrap_or(0);
    WorkflowSummary {
        id: b.id,
        tn_id: b.transformation_novel_id,
        label: b.label.clone(),
        status: b.status,
        created_at: b.created_at.to_rfc3339(),
        started_at: b.started_at.map(|t| t.to_rfc3339()),
        ended_at: b.ended_at.map(|t| t.to_rfc3339()),
        done_count: done,
        failed_count: failed,
        skipped_count: skipped,
        total_count: total,
        promoted_count,
    }
}

#[tauri::command]
pub async fn create_workflow(
    db: State<'_, Arc<Mutex<Db>>>,
    payload: CreateWorkflowPayload,
    scheduler: State<'_, Arc<BatchScheduler>>,
) -> Result<WorkflowSummary, String> {
    let sched = scheduler.inner().clone();
    let spec = payload.into_core().map_err(|e| e.to_string())?;
    let res = tokio::task::spawn_blocking(move || sched.create_workflow(spec))
        .await
        .map_err(|e| format!("create_workflow join: {e}"))?
        .map_err(|e| e.to_string())?;
    let db = db.lock().map_err(|e| e.to_string())?;
    Ok(to_summary(&db, &res))
}

#[tauri::command]
pub fn list_workflows(
    db: State<'_, Arc<Mutex<Db>>>,
    tn_id: i64,
) -> Result<Vec<WorkflowSummary>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let batches = db.batches().list_by_tn(tn_id).map_err(|e| e.to_string())?;
    Ok(batches.iter().map(|b| to_summary(&db, b)).collect())
}

#[tauri::command]
pub fn get_workflow(
    db: State<'_, Arc<Mutex<Db>>>,
    batch_id: i64,
) -> Result<WorkflowSummary, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let b = db.batches().get(batch_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("batch {batch_id} 不存在"))?;
    Ok(to_summary(&db, &b))
}

#[tauri::command]
pub fn list_workflow_chapters(
    db: State<'_, Arc<Mutex<Db>>>,
    batch_id: i64,
) -> Result<Vec<WorkflowChapterRow>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let _batch = db.batches().get(batch_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("batch {batch_id} 不存在"))?;

    let mut stmt = db.conn.prepare(
        "SELECT tc.id, tc.chapter_id, c.idx, c.title, tc.status, tc.error, wrc.content \
         FROM transformation_chapters tc \
         JOIN chapters c ON c.id = tc.chapter_id \
         LEFT JOIN workflow_result_chapters wrc \
            ON wrc.chapter_id = tc.chapter_id \
            AND wrc.workflow_result_id = (SELECT id FROM workflow_results WHERE batch_id = ?1) \
         WHERE tc.batch_id = ?1 \
         ORDER BY c.idx ASC, tc.id ASC",
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(rusqlite::params![batch_id], |row| {
        let status_s: String = row.get(4)?;
        let status = parse_transform_status(&status_s).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, e.into())
        })?;
        let content: Option<String> = row.get(6)?;
        let preview = content.as_deref().map(preview_first_chars);
        let is_empty = content.is_none();
        Ok(WorkflowChapterRow {
            tc_id: row.get(0)?,
            chapter_id: row.get(1)?,
            chapter_idx: row.get(2)?,
            chapter_title: row.get(3)?,
            status,
            error: row.get(5)?,
            content_preview: preview,
            is_empty_slot: is_empty,
        })
    }).map_err(|e| e.to_string())?;
    let collected: Vec<WorkflowChapterRow> = rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(collected)
}

fn preview_first_chars(s: &str) -> String {
    s.chars().take(CONTENT_PREVIEW_CHARS).collect()
}

#[tauri::command]
pub async fn stop_workflow(
    db: State<'_, Arc<Mutex<Db>>>,
    batch_id: i64,
    scheduler: State<'_, Arc<BatchScheduler>>,
) -> Result<WorkflowSummary, String> {
    let sched = scheduler.inner().clone();
    let res = tokio::task::spawn_blocking(move || sched.stop_workflow(batch_id))
        .await
        .map_err(|e| format!("stop_workflow join: {e}"))?
        .map_err(|e| e.to_string())?;
    let db = db.lock().map_err(|e| e.to_string())?;
    Ok(to_summary(&db, &res))
}

#[tauri::command]
pub async fn retry_workflow_chapters(
    db: State<'_, Arc<Mutex<Db>>>,
    batch_id: i64,
    chapter_ids: Vec<i64>,
    scheduler: State<'_, Arc<BatchScheduler>>,
) -> Result<WorkflowSummary, String> {
    let sched = scheduler.inner().clone();
    let res = tokio::task::spawn_blocking(move || sched.retry_empty_slots(batch_id, &chapter_ids))
        .await
        .map_err(|e| format!("retry_workflow_chapters join: {e}"))?
        .map_err(|e| e.to_string())?;
    let db = db.lock().map_err(|e| e.to_string())?;
    Ok(to_summary(&db, &res))
}

#[tauri::command]
pub fn list_transformation_source_chapters(
    db: State<'_, Arc<Mutex<Db>>>,
    tn_id: i64,
) -> Result<Vec<SourceChapterRow>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let tn = db.transformation_novels().get(tn_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("tn {tn_id} 不存在"))?;
    let mut stmt = db.conn.prepare(
        "SELECT c.id, c.idx, c.title, c.word_count, \
                COALESCE((SELECT COUNT(*) FROM workflow_result_chapters wrc \
                    JOIN workflow_results wr ON wr.id = wrc.workflow_result_id \
                    JOIN batches b ON b.id = wr.batch_id \
                    WHERE b.transformation_novel_id = ?1 \
                      AND wrc.chapter_id = c.id \
                      AND wrc.content IS NOT NULL), 0) \
         FROM chapters c \
         WHERE c.data_asset_id = ?2 \
         ORDER BY c.idx ASC",
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(
        rusqlite::params![tn_id, tn.data_asset_id],
        |row| Ok(SourceChapterRow {
            chapter_id: row.get(0)?,
            idx: row.get(1)?,
            title: row.get(2)?,
            word_count: row.get(3)?,
            non_empty_result_count: row.get(4)?,
        }),
    ).map_err(|e| e.to_string())?;
    let collected: Vec<SourceChapterRow> = rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(collected)
}

#[tauri::command]
pub fn list_chapter_workflow_results(
    db: State<'_, Arc<Mutex<Db>>>,
    tn_id: i64,
    chapter_id: i64,
) -> Result<Vec<ChapterWorkflowResultRow>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let mut stmt = db.conn.prepare(
        "SELECT b.id, b.label, b.status, b.ended_at, wrc.content, tc.status \
         FROM batches b \
         JOIN workflow_results wr ON wr.batch_id = b.id \
         JOIN workflow_result_chapters wrc ON wrc.workflow_result_id = wr.id \
         LEFT JOIN transformation_chapters tc \
            ON tc.batch_id = b.id AND tc.chapter_id = wrc.chapter_id \
         WHERE b.transformation_novel_id = ?1 AND wrc.chapter_id = ?2 \
         ORDER BY b.id DESC",
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(
        rusqlite::params![tn_id, chapter_id],
        |row| {
            let batch_status_s: String = row.get(2)?;
            let batch_status = parse_batch_status(&batch_status_s).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, e.into())
            })?;
            let tc_status_s: Option<String> = row.get(5)?;
            let tc_status = match tc_status_s.as_deref() {
                None => TransformStatus::Pending,
                Some(s) => parse_transform_status(s).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, e.into())
                })?,
            };
            Ok(ChapterWorkflowResultRow {
                batch_id: row.get(0)?,
                batch_label: row.get(1)?,
                batch_status,
                batch_ended_at: row.get(3)?,
                content: row.get(4)?,
                status: tc_status,
            })
        },
    ).map_err(|e| e.to_string())?;
    let collected: Vec<ChapterWorkflowResultRow> = rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(collected)
}

#[tauri::command]
pub async fn regenerate_chapter_preview(
    scheduler: State<'_, Arc<BatchScheduler>>,
    batch_id: i64,
    chapter_id: i64,
    custom_input: Option<String>,
) -> Result<i64, String> {
    let sched = scheduler.inner().clone();
    let res = tokio::task::spawn_blocking(move || {
        sched.regenerate_preview(batch_id, chapter_id, custom_input)
    })
    .await
    .map_err(|e| format!("regenerate_chapter_preview join: {e}"))?
    .map_err(|e| e.to_string())?;
    Ok(res)
}

#[tauri::command]
pub fn commit_chapter_preview(
    db: State<'_, Arc<Mutex<Db>>>,
    scheduler: State<'_, Arc<BatchScheduler>>,
    input: CommitPreviewInput,
) -> Result<WorkflowSummary, String> {
    let sched = scheduler.inner().clone();
    let res = sched.commit_preview(
        input.batch_id,
        input.chapter_id,
        input.draft_content,
        input.source_preview_id,
    ).map_err(|e| e.to_string())?;
    let db = db.lock().map_err(|e| e.to_string())?;
    Ok(to_summary(&db, &res))
}

#[tauri::command]
pub fn list_chapter_previews(
    scheduler: State<'_, Arc<BatchScheduler>>,
    batch_id: i64,
    chapter_id: i64,
) -> Result<Vec<nsc_core::models::ChapterPreviewRow>, String> {
    let sched = scheduler.inner().clone();
    sched.list_chapter_previews(batch_id, chapter_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn discard_chapter_preview(
    scheduler: State<'_, Arc<BatchScheduler>>,
    preview_id: i64,
) -> Result<(), String> {
    let sched = scheduler.inner().clone();
    sched.discard_preview(preview_id).map_err(|e| e.to_string())
}
