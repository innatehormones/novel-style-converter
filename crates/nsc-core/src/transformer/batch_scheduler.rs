//! 批号调度器:按 frontier 串行派发,跨工作流不共享结果。
//!
//! 单例;持 `db_path`(不在 Db 上 Sync);由 lib.rs 在 JobQueue::set_notifier 时注册。
//!
//! 本片接:
//! - `create_workflow` 原子事务:batch + workflow_results + N 个 tc + N 个空 slot
//! - `on_chapter_done` / `on_chapter_failed` 派下一章(`on_chapter_failed` 按 batch.on_failure_policy 分流)
//! - 完成判据 → batch 状态迁移到 Stopped/Terminated 等
//! - `safe_stop_on_dispatch_failure` dispatch 失败的兜底
//! - `stop_workflow` 人工停止 + `retry_empty_slots` 重试空槽
//! - `resume` 配合 pause_and_review 策略,让用户在 paused 时介入

use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use rusqlite::OptionalExtension;

use crate::db::Db;
use crate::error::{Error, Result};
use crate::models::{
    Batch, BatchStatus, Chapter, ChapterPreviewRow, ModelConfig, OnFailurePolicy, Prompt,
    PromptKind, ResumeAction, TransformationNovel,
};
use crate::models::AiCallBusiness;
use crate::recorder::AiCallRecorder;
use crate::transformer::{
    DefaultTransformer, JobQueue, JobSpec, ProviderFactory, TransformRequest,
    TransformationNovelContext,
};

pub struct BatchScheduler {
    db_path: PathBuf,
    job_queue: Arc<JobQueue>,
    provider_factory: ProviderFactory,
    recorder: Arc<dyn AiCallRecorder>,
    /// 后台 tokio runtime —— regenerate_preview 的 AI 任务在这里跑
    /// (主线程没 tokio reactor,见 src-tauri/src/lib.rs:51 注释)。
    runtime: Arc<tokio::runtime::Runtime>,
}

/// `create_workflow` 入参 —— 字段全是必填,不走任何 TN 默认覆盖。
#[derive(Debug, Clone)]
pub struct WorkflowCreate {
    pub transformation_novel_id: i64,
    pub label: Option<String>,
    pub chapter_ids: Vec<i64>,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub mode: PromptKind,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
    /// 章节失败时的处理策略。
    /// - PauseAndReview: batch → Paused,等用户通过 `resume` 决策。
    /// - Terminate: 同 batch 后续 pending → cancelled,batch → Terminated。
    /// - SkipFailed: 当前 tc → Skipped,继续派下一章(batch 留 Running)。
    pub on_failure_policy: OnFailurePolicy,
}

impl BatchScheduler {
    pub fn new(
        db_path: PathBuf,
        job_queue: Arc<JobQueue>,
        provider_factory: ProviderFactory,
        recorder: Arc<dyn AiCallRecorder>,
    ) -> Self {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
                .expect("BatchScheduler runtime build"),
        );
        Self { db_path, job_queue, provider_factory, recorder, runtime }
    }

    /// 原子创建并启动工作流(spec §5.1,§12):单事务里写 batches(status='running',started_at=now) +
    /// workflow_results + N × transformation_chapters(status='pending') +
    /// N × 空 workflow_result_chapters;事务提交后立刻 advance_batch 派首章。
    /// 派首章失败时 safe_stop:所有 pending tc → failed,batch → stopped。
    /// 字段全是必填,不回退任何 TN 默认。
    pub fn create_workflow(&self, spec: WorkflowCreate) -> Result<Batch> {
        if spec.chapter_ids.is_empty() {
            return Err(Error::Validation("必须选择至少一个章节".into()));
        }
        let db = Db::connect(&self.db_path)?;

        // 1. 校验 TN / chapter 归属 / prompt / model / mode↔prompt.kind 一致
        let tn = db.transformation_novels().get(spec.transformation_novel_id)?
            .ok_or_else(|| Error::NotFound(format!("tn {} 不存在", spec.transformation_novel_id)))?;
        for cid in &spec.chapter_ids {
            let ch = db.chapters().get(*cid)?
                .ok_or_else(|| Error::NotFound(format!("chapter {cid} 不存在")))?;
            if ch.data_asset_id != tn.data_asset_id {
                return Err(Error::Validation(format!(
                    "chapter {cid} 不属于 tn 的 data_asset {}", tn.data_asset_id
                )));
            }
        }
        let prompt = db.prompts().get(spec.prompt_id)?
            .ok_or_else(|| Error::NotFound(format!("prompt {} 不存在", spec.prompt_id)))?;
        // model_config 只用来校验存在性;create_workflow 事务提交后立刻按 tc 行的 model_config_id 派首章。
        let _model = db.model_configs().get(spec.model_config_id)?
            .ok_or_else(|| Error::NotFound(format!("model_config {} 不存在", spec.model_config_id)))?;
        if PromptKind::from(prompt.kind) != spec.mode {
            return Err(Error::Validation("prompt kind 与 mode 不一致".into()));
        }

        // 2. 单事务:batch(status=running,started_at=now) + 结果集 + N × tc + N × 空槽
        let now = Utc::now().to_rfc3339();
        let batch_id: i64 = {
            let tx = db.conn.unchecked_transaction()?;
            tx.execute(
                "INSERT INTO batches (transformation_novel_id, label, on_failure_policy, status, created_at, started_at)                  VALUES (?1, ?2, ?3, 'running', ?4, ?4)",
                rusqlite::params![
                    spec.transformation_novel_id, spec.label,
                    policy_str(spec.on_failure_policy), now,
                ],
            )?;
            let batch_id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO workflow_results (batch_id, created_at) VALUES (?1, ?2)",
                rusqlite::params![batch_id, now],
            )?;
            let result_id: i64 = tx.query_row(
                "SELECT id FROM workflow_results WHERE batch_id = ?1",
                rusqlite::params![batch_id], |r| r.get(0),
            )?;
            for cid in &spec.chapter_ids {
                let frontier_cid = frontier_chapter_id_in_workflow(&tx, batch_id, *cid)?;
                tx.execute(
                    "INSERT INTO transformation_chapters                      (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id,                       ctx_prev_original, ctx_prev_transformed, ctx_next_original,                       batch_id, style_ref_chapter_id, status)                      VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending')",
                    rusqlite::params![
                        spec.transformation_novel_id, *cid, mode_str(spec.mode),
                        spec.prompt_id, spec.model_config_id,
                        spec.ctx_prev_original, spec.ctx_prev_transformed, spec.ctx_next_original,
                        batch_id,
                        frontier_cid,
                    ],
                )?;
                tx.execute(
                    "INSERT INTO workflow_result_chapters                      (workflow_result_id, chapter_id, content, created_at, updated_at)                      VALUES (?1, ?2, NULL, ?3, ?3)",
                    rusqlite::params![result_id, cid, now],
                )?;
            }
            tx.commit()?;
            batch_id
        };

        // 创建即运行(spec §12):事务提交后立刻 advance_batch 派首章。
        // 派首章失败时 safe_stop,所有 pending tc → failed,batch → stopped。
        if let Err(e) = self.advance_batch(&db, batch_id) {
            let now2 = Utc::now().to_rfc3339();
            let tx2 = db.conn.unchecked_transaction()?;
            tx2.execute(
                "UPDATE transformation_chapters                  SET status='failed', error=?2, completed_at=?3, result_content=NULL,                      tokens_in=NULL, tokens_out=NULL                  WHERE batch_id=?1 AND status='pending'",
                rusqlite::params![batch_id, format!("create_workflow 派首章失败: {e}"), now2],
            )?;
            tx2.execute(
                "UPDATE batches SET status='stopped', ended_at=?1 WHERE id=?2",
                rusqlite::params![now2, batch_id],
            )?;
            tx2.commit()?;
            return Err(e);
        }

        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound("batch 写入后回读失败".into()))?;
        Ok(batch)
    }

    /// 派发一个具体 tc(按 tid)。从 Db 读 chapter,构造 JobSpec 塞进 JobQueue。
    pub(crate) fn dispatch(
        &self,
        db: &Db,
        _tn: &TransformationNovel,
        prompt: &Prompt,
        model: &ModelConfig,
        tid: i64,
        ctx_prev_original: i32,
        ctx_prev_transformed: i32,
        ctx_next_original: i32,
    ) -> Result<()> {
        let tc = db.transformation_chapters().get(tid)?
            .ok_or_else(|| Error::NotFound(format!("tc {tid} 不存在")))?;
        let chapter = db.chapters().get(tc.chapter_id)?
            .ok_or_else(|| Error::NotFound(format!("chapter {} 不存在", tc.chapter_id)))?;

        let spec = JobSpec {
            // tc.transformation_novel_id 才是 transformation_novels 表的 id；
            // JobQueue / Transformer 根据这个 id 读 tn，不能实际为 transformation_chapters.id (tid)。
            tc_id: tid,
            tn_id: tc.transformation_novel_id,
            // tc.mode 由 `create_workflow` 在 tc 行 INSERT 时写入,是 per-task 的权威值。
            mode: tc.mode,
            chapter: Chapter {
                id: chapter.id,
                data_asset_id: chapter.data_asset_id,
                idx: chapter.idx,
                title: chapter.title.clone(),
                body: chapter.body.clone(),
                word_count: chapter.word_count,
                source_kind: chapter.source_kind.clone(),
                source_chapter_id: chapter.source_chapter_id,
                edited_at: chapter.edited_at.clone(),
            },
            prompt: prompt.clone(),
            model_config: model.clone(),
            ctx_prev_original,
            ctx_prev_transformed,
            ctx_next_original,
        };
        self.job_queue.enqueue(spec);
        Ok(())
    }

    /// JobQueue 完成回调:把正文写入结果集 + 派下一章(若还有)。
    /// 单事务里标 tc done(清空 `tc.result_content` 回到结果槽),同步写
    /// `workflow_result_chapters.content`,然后 `advance_batch`。
    pub fn on_chapter_done(&self, tid: i64, content: String) -> Result<()> {
        let db = Db::open(&self.db_path)?;
        let tc = db.transformation_chapters().get(tid)?
            .ok_or_else(|| Error::NotFound(format!("tc {tid} 不存在")))?;
        let batch_id = match tc.batch_id {
            Some(b) => b,
            None => return Ok(()),  // 散点行(非 batch 入队)不归 scheduler 管
        };
        let now = Utc::now().to_rfc3339();
        {
            let tx = db.conn.unchecked_transaction()?;
            // tc 行:保留已由 worker 写入的 tokens_in/out,清空 result_content(spec §5.x 收口到结果集)。
            tx.execute(
                "UPDATE transformation_chapters                  SET result_content=NULL, completed_at=?1                  WHERE id=?2",
                rusqlite::params![now, tid],
            )?;
            // 同步写结果槽 —— `WorkflowResultRepo::write_content_by_chapter` 通过
            // sub-select 找 workflow_results.id,对未建结果集 / 缺槽的 batch 静默 noop,
            // 让老 batch(非工作流)路径也能调到这里而不报错。
            tx.execute(
                "UPDATE workflow_result_chapters                  SET content=?2, updated_at=?3                  WHERE workflow_result_id = (SELECT id FROM workflow_results WHERE batch_id=?4)                    AND chapter_id=?1",
                rusqlite::params![tc.chapter_id, content, now, batch_id],
            )?;
            tx.commit()?;
        }
        self.advance_batch(&db, batch_id)
    }

    /// 失败回调:按 batch.on_failure_policy 分流。
    /// - PauseAndReview: tc → failed,batch → paused(ended_at 设 NOW),不 advance。
    /// - Terminate:       tc → failed;同 batch 后续 pending → cancelled,batch → terminated(不 advance)。
    /// - SkipFailed:      tc → skipped,advance_batch 派下一章(batch 保持 running)。
    /// batch 收尾交给 advance_batch → maybe_finalize_batch(skip_failed 走这条)。
    pub fn on_chapter_failed(&self, tid: i64, error: String) -> Result<()> {
        let db = Db::open(&self.db_path)?;
        let tc = db.transformation_chapters().get(tid)?
            .ok_or_else(|| Error::NotFound(format!("tc {tid} 不存在")))?;
        let Some(batch_id) = tc.batch_id else { return Ok(()); };
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        let now = Utc::now().to_rfc3339();
        match batch.on_failure_policy {
            OnFailurePolicy::PauseAndReview => {
                let tx = db.conn.unchecked_transaction()?;
                tx.execute(
                    "UPDATE transformation_chapters                      SET status='failed', error=?2, completed_at=?3, result_content=NULL,                          tokens_in=NULL, tokens_out=NULL                      WHERE id=?1",
                    rusqlite::params![tid, error, now],
                )?;
                tx.execute(
                    "UPDATE batches SET status='paused', ended_at=?1 WHERE id=?2",
                    rusqlite::params![now, batch_id],
                )?;
                tx.commit()?;
                Ok(())
            }
            OnFailurePolicy::Terminate => {
                let tx = db.conn.unchecked_transaction()?;
                tx.execute(
                    "UPDATE transformation_chapters                      SET status='failed', error=?2, completed_at=?3, result_content=NULL,                          tokens_in=NULL, tokens_out=NULL                      WHERE id=?1",
                    rusqlite::params![tid, error, now],
                )?;
                tx.execute(
                    "UPDATE transformation_chapters SET status='cancelled'                      WHERE batch_id=?1 AND status='pending'",
                    rusqlite::params![batch_id],
                )?;
                tx.execute(
                    "UPDATE batches SET status='terminated', ended_at=?1 WHERE id=?2",
                    rusqlite::params![now, batch_id],
                )?;
                tx.commit()?;
                Ok(())
            }
            OnFailurePolicy::SkipFailed => {
                let tx = db.conn.unchecked_transaction()?;
                tx.execute(
                    "UPDATE transformation_chapters                      SET status='skipped', error=?2, completed_at=?3, result_content=NULL,                          tokens_in=NULL, tokens_out=NULL                      WHERE id=?1",
                    rusqlite::params![tid, error, now],
                )?;
                tx.commit()?;
                self.advance_batch(&db, batch_id)
            }
        }
    }

    /// 派下一章(若有);完成判据。
    fn advance_batch(&self, db: &Db, batch_id: i64) -> Result<()> {
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;

        // 取 batch 内第一个 pending 行(按 chapter_idx ASC)
        let next_tid: Option<i64> = {
            let mut stmt = db.conn.prepare(
                "SELECT transformation_chapters.id FROM transformation_chapters                  JOIN chapters c ON c.id = transformation_chapters.chapter_id                  WHERE transformation_chapters.batch_id = ?1                    AND transformation_chapters.status = 'pending'                  ORDER BY c.idx ASC, transformation_chapters.id ASC                  LIMIT 1",
            )?;
            let mut rows = stmt.query(rusqlite::params![batch_id])?;
            if let Some(row) = rows.next()? { Some(row.get(0)?) } else { None }
        };

        if let Some(tid) = next_tid {
            // 还有 pending → 派下一章。prompt_id / model_config_id 从 tc 行直接读,
            // 跟 create_workflow 派首章对齐:WorkflowCreate.prompt_id/model_config_id
            // 在事务里已经写进每个 tc 行(`INSERT ... prompt_id, model_config_id`),
            // 无需任何 TN 层 fallback。
            let tn_id = batch.transformation_novel_id;
            let tn = db.transformation_novels().get(tn_id)?
                .ok_or_else(|| Error::NotFound(format!("tn {tn_id} 不存在")))?;
            let next_tc = db.transformation_chapters().get(tid)?
                .ok_or_else(|| Error::NotFound(format!("tc {tid} 不存在")))?;
            let prompt_id = next_tc.prompt_id;
            let model_cfg_id = next_tc.model_config_id;
            let prompt = db.prompts().get(prompt_id)?
                .ok_or_else(|| Error::NotFound(format!("prompt {prompt_id} 不存在")))?;
            let model = db.model_configs().get(model_cfg_id)?
                .ok_or_else(|| Error::NotFound(format!("model_config {model_cfg_id} 不存在")))?;
            return self.dispatch(db, &tn, &prompt, &model, tid, 0, 0, 0);
        }

        // 没 pending 了 → 完成判据
        self.maybe_finalize_batch(db, batch_id)
    }

    /// §3.3 / §5.2 收尾判据:批次内不存在 pending/running 任务 → batch → Stopped。
    /// Failed/Done/Skipped/Cancelled 都不阻塞收尾。
    /// COALESCE(ended_at, ?1) 保留已有 ended_at(如 Task 7 手动停止写入的)。
    fn maybe_finalize_batch(&self, db: &Db, batch_id: i64) -> Result<()> {
        let active: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM transformation_chapters              WHERE batch_id = ?1 AND status IN ('pending','running')",
            rusqlite::params![batch_id],
            |row| row.get(0),
        )?;
        if active > 0 {
            return Ok(());
        }
        let now = Utc::now().to_rfc3339();
        db.conn.execute(
            "UPDATE batches SET status='stopped', ended_at = COALESCE(ended_at, ?1) WHERE id = ?2",
            rusqlite::params![now, batch_id],
        )?;
        Ok(())
    }

    /// 用户在 paused 时介入。三种动作:
    ///   Retry(ch_id):    tc 重置为 pending + 立即 dispatch(从 tc 行读 prompt/model,不走任何 TN 默认)
    ///   Skip(ch_id):     tc 标 skipped + dispatch 下一章
    ///   Terminate:       同 batch 后续 pending → cancelled, batch Terminated
    pub fn resume(&self, batch_id: i64, action: ResumeAction) -> Result<Batch> {
        let db = Db::open(&self.db_path)?;
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        if !matches!(batch.status, BatchStatus::Paused) {
            return Err(Error::Validation(format!(
                "batch {batch_id} 不是 Paused(当前 {:?}),不能 resume",
                batch.status
            )));
        }

        let now = Utc::now().to_rfc3339();
        let tx = db.conn.unchecked_transaction()?;
        match action {
            ResumeAction::Retry(ch_id) => {
                tx.execute(
                    "UPDATE transformation_chapters                      SET status='pending', result_content=NULL, tokens_in=NULL, tokens_out=NULL,                          error=NULL, started_at=NULL, completed_at=NULL                      WHERE id=?1 AND batch_id=?2",
                    rusqlite::params![ch_id, batch_id],
                )?;
                tx.execute(
                    "UPDATE batches SET status='running', ended_at=NULL WHERE id=?1",
                    rusqlite::params![batch_id],
                )?;
                tx.commit()?;

                // 立即 dispatch this ch:从 tc 行读固化好的 prompt/model
                // (跟 create_workflow 派首章 / advance_batch 派下一章对齐)。
                let tn_id = batch.transformation_novel_id;
                let tn = db.transformation_novels().get(tn_id)?
                    .ok_or_else(|| Error::NotFound(format!("tn {tn_id} 不存在")))?;
                let tc = db.transformation_chapters().get(ch_id)?
                    .ok_or_else(|| Error::NotFound(format!("tc {ch_id} 不存在")))?;
                let prompt_id = tc.prompt_id;
                let model_id = tc.model_config_id;
                let prompt = db.prompts().get(prompt_id)?
                    .ok_or_else(|| Error::NotFound(format!("prompt {prompt_id} 不存在")))?;
                let model = db.model_configs().get(model_id)?
                    .ok_or_else(|| Error::NotFound(format!("model_config {model_id} 不存在")))?;
                self.dispatch(&db, &tn, &prompt, &model, ch_id, 0, 0, 0)?;
            }
            ResumeAction::Skip(ch_id) => {
                tx.execute(
                    "UPDATE transformation_chapters SET status='skipped', completed_at=?2                      WHERE id=?1 AND batch_id=?3",
                    rusqlite::params![ch_id, now, batch_id],
                )?;
                tx.execute(
                    "UPDATE batches SET status='running', ended_at=NULL WHERE id=?1",
                    rusqlite::params![batch_id],
                )?;
                tx.commit()?;
                self.advance_batch(&db, batch_id)?;
            }
            ResumeAction::Terminate => {
                tx.execute(
                    "UPDATE transformation_chapters SET status='cancelled'                      WHERE batch_id=?1 AND status='pending'",
                    rusqlite::params![batch_id],
                )?;
                tx.execute(
                    "UPDATE batches SET status='terminated', ended_at=?1 WHERE id=?2",
                    rusqlite::params![now, batch_id],
                )?;
                tx.commit()?;
            }
        }
        let b = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound("batch 回读失败".into()))?;
        Ok(b)
    }

    /// 人工停止运行中的工作流(spec §6.1):事务里把全部 Pending 标 Skipped、结果槽保持空;
    /// 若当时没有 Running 任务,批次直接转 Stopped,否则等 worker 回调 finalize。
    /// 对已 Stopped 批次幂等返回(spec §10);其余状态(非 Running、非 Stopped)→ Validation。
    pub fn stop_workflow(&self, batch_id: i64) -> Result<Batch> {
        let db = Db::open(&self.db_path)?;
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        if matches!(batch.status, BatchStatus::Stopped) {
            return Ok(batch);
        }
        if !matches!(batch.status, BatchStatus::Running) {
            return Err(Error::Validation(format!(
                "batch {batch_id} 状态 {:?} 不能 stop", batch.status
            )));
        }
        let now = Utc::now().to_rfc3339();
        let tx = db.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE transformation_chapters SET status='skipped', completed_at=?2              WHERE batch_id=?1 AND status='pending'",
            rusqlite::params![batch_id, now],
        )?;
        let has_running: i64 = tx.query_row(
            "SELECT COUNT(*) FROM transformation_chapters WHERE batch_id=?1 AND status='running'",
            rusqlite::params![batch_id], |r| r.get(0),
        )?;
        if has_running == 0 {
            tx.execute(
                "UPDATE batches SET status='stopped', ended_at=?1 WHERE id=?2",
                rusqlite::params![now, batch_id],
            )?;
        }
        tx.commit()?;
        let updated = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound("batch 回读失败".into()))?;
        Ok(updated)
    }

    /// Stopped 后重试空槽(spec §6.2):把所选 Failed/Skipped 任务重置为 Pending,
    /// 仅当结果槽为 NULL 时通过;事务提交后 batch → Running,派序号最小的章节。
    /// 已被填过的槽重试会返回 Validation(spec §10)。
    pub fn retry_empty_slots(&self, batch_id: i64, chapter_ids: &[i64]) -> Result<Batch> {
        let db = Db::open(&self.db_path)?;
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        // Stopped: 原语义；Running/Paused: 允许在没有其他 running tc 的前提下对失败/跳过单章重试,避免重复 dispatch。
        let in_flight: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM transformation_chapters WHERE batch_id=?1 AND status='running'",
            rusqlite::params![batch_id], |r| r.get(0),
        )?;
        match batch.status {
            BatchStatus::Stopped => {}
            BatchStatus::Running | BatchStatus::Paused if in_flight == 0 => {}
            _ => return Err(Error::Validation(format!(
                "当前 batch 状态不可重试（仅允许 Stopped 或无 in-flight 的 Running/Paused）"
            ))),
        }
        if chapter_ids.is_empty() {
            return Err(Error::Validation("必须至少选择一个章节".into()));
        }
        let first_tid: i64 = {
            let tx = db.conn.unchecked_transaction()?;
            for cid in chapter_ids {
                let updated = tx.execute(
                    "UPDATE transformation_chapters                      SET status='pending', error=NULL, result_content=NULL,                          tokens_in=NULL, tokens_out=NULL, started_at=NULL, completed_at=NULL                      WHERE batch_id=?1                        AND chapter_id=?2                        AND status IN ('failed','skipped')                        AND (SELECT content FROM workflow_result_chapters wrc                              JOIN workflow_results wr ON wr.id = wrc.workflow_result_id                              WHERE wr.batch_id = transformation_chapters.batch_id                                AND wrc.chapter_id = transformation_chapters.chapter_id) IS NULL",
                    rusqlite::params![batch_id, cid],
                )?;
                if updated == 0 {
                    return Err(Error::Validation(format!(
                        "章节 {cid} 不是可重试空槽(不存在/非 failed-skipped/结果槽非空)"
                    )));
                }
            }
            if matches!(batch.status, BatchStatus::Stopped) {
                tx.execute(
                    "UPDATE batches SET status='running', ended_at=NULL WHERE id=?1",
                    rusqlite::params![batch_id],
                )?;
            }
            let first_tid: i64 = tx.query_row(
                "SELECT tc.id FROM transformation_chapters tc                  JOIN chapters c ON c.id = tc.chapter_id                  WHERE tc.batch_id=?1 AND tc.status='pending'                  ORDER BY c.idx ASC LIMIT 1",
                rusqlite::params![batch_id], |r| r.get(0),
            )?;
            tx.commit()?;
            first_tid
        };
        // 派首章(事务外):从 tc 行读固化好的 prompt/model(跟 create_workflow 对齐)。
        let tn = db.transformation_novels().get(batch.transformation_novel_id)?
            .ok_or_else(|| Error::NotFound(format!("tn {} 不存在", batch.transformation_novel_id)))?;
        let prompt_id: i64 = db.conn.query_row(
            "SELECT prompt_id FROM transformation_chapters WHERE id=?1",
            rusqlite::params![first_tid], |r| r.get(0),
        )?;
        let model_id: i64 = db.conn.query_row(
            "SELECT model_config_id FROM transformation_chapters WHERE id=?1",
            rusqlite::params![first_tid], |r| r.get(0),
        )?;
        let prompt = db.prompts().get(prompt_id)?
            .ok_or_else(|| Error::NotFound(format!("prompt {prompt_id} 不存在")))?;
        let model = db.model_configs().get(model_id)?
            .ok_or_else(|| Error::NotFound(format!("model {model_id} 不存在")))?;
        self.dispatch(&db, &tn, &prompt, &model, first_tid, 0, 0, 0)?;
        let updated = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound("batch 回读失败".into()))?;
        Ok(updated)
    }

    /// 测试用:当前 batch 状态(方便断言)。
    pub fn batch_status(&self, batch_id: i64) -> Result<BatchStatus> {
        let db = Db::open(&self.db_path)?;
        let b = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        Ok(b.status)
    }

    /// 异步发起一次预览生成(spec §5.1)。返回新插入的 preview id。
    /// AI 调用在 self.runtime 上跑 —— 调用方立即拿到 preview_id,
    /// 实际生成在后台进行,完成后由 chapter_previews 行 status 反映。
    pub fn regenerate_preview(
        &self,
        batch_id: i64,
        chapter_id: i64,
        custom_input: Option<String>,
    ) -> Result<i64> {
        let db = Db::connect(&self.db_path)?;
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        let chapter = db.chapters().get(chapter_id)?
            .ok_or_else(|| Error::NotFound(format!("chapter {chapter_id} 不存在")))?;
        let tn = db.transformation_novels().get(batch.transformation_novel_id)?
            .ok_or_else(|| Error::NotFound(format!("tn {} 不存在", batch.transformation_novel_id)))?;
        let (prompt_id, model_config_id): (i64, i64) = db.conn.query_row(
            "SELECT prompt_id, model_config_id FROM transformation_chapters \
             WHERE batch_id=?1 AND chapter_id=?2",
            rusqlite::params![batch_id, chapter_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let prompt = db.prompts().get(prompt_id)?
            .ok_or_else(|| Error::NotFound(format!("prompt {prompt_id} 不存在")))?;
        let model = db.model_configs().get(model_config_id)?
            .ok_or_else(|| Error::NotFound(format!("model {model_config_id} 不存在")))?;
        if chapter.data_asset_id != tn.data_asset_id {
            return Err(Error::Validation(format!(
                "chapter {chapter_id} 不属于 tn 的 data_asset {}",
                tn.data_asset_id
            )));
        }

        let preview_id = db.chapter_previews()
            .insert_generating(batch_id, chapter_id, custom_input.as_deref())?;

        let req = TransformRequest {
            transformation_id: batch.transformation_novel_id,
            chapter: chapter.clone(),
            chapter_content: chapter.body.clone(),
            novel_context: TransformationNovelContext {
                transformation_novel: tn.clone(),
                prev_original: Vec::new(),
                prev_transformed: Vec::new(),
                next_original: Vec::new(),
            },
            prompt: prompt.clone(),
            model_config: model.clone(),
            custom_input: custom_input.clone(),
            preview_id: Some(preview_id),
        };

        let provider = (self.provider_factory)(&model);
        let recorder = self.recorder.clone();
        let db_path = self.db_path.clone();
        self.runtime.spawn(async move {
            let tx = DefaultTransformer { ai: provider.into(), recorder: recorder.clone() };
            let result = tx.transform_with_business(req, AiCallBusiness::RegeneratePreview).await;
            let update_result = (|| -> Result<()> {
                let db = Db::connect(&db_path)?;
                match &result {
                    Ok(out) => db.chapter_previews().update_done(
                        preview_id, &out.result_content, out.tokens_in, out.tokens_out,
                    )?,
                    Err(e) => db.chapter_previews().update_failed(preview_id, &e.to_string())?,
                }
                Ok(())
            })();
            if let Err(e) = update_result {
                eprintln!("[regenerate_preview] 更新 preview {preview_id} 失败: {e}");
            }
        });

        Ok(preview_id)
    }

    /// 列出 (batch_id, chapter_id) 下的全部 preview 行,按 id DESC —— spec §5.1。
    pub fn list_chapter_previews(&self, batch_id: i64, chapter_id: i64) -> Result<Vec<ChapterPreviewRow>> {
        let db = Db::open(&self.db_path)?;
        db.chapter_previews().list_by_chapter(batch_id, chapter_id)
    }

    /// 放弃某个 preview 行(直接删除,不管 status;generating 也允许)—— spec §5.1。
    pub fn discard_preview(&self, preview_id: i64) -> Result<()> {
        let db = Db::open(&self.db_path)?;
        db.chapter_previews().delete(preview_id)
    }

    /// 用草稿区内容覆写 wrc.content(spec §4.2):单事务里写 wrc.content +
    /// tc.status='done' + 更新 tc.tokens(优先 source_preview_id,fallback 最新一条 done,都没有则 NULL)
    /// + 清空该章节所有 preview 行。不修改 batch 状态(spec §7.1)。
    pub fn commit_preview(
        &self,
        batch_id: i64,
        chapter_id: i64,
        draft_content: String,
        source_preview_id: Option<i64>,
    ) -> Result<Batch> {
        let db = Db::connect(&self.db_path)?;
        let now = Utc::now().to_rfc3339();
        {
            let tx = db.conn.unchecked_transaction()?;
            let tc_id: i64 = tx.query_row(
                "SELECT id FROM transformation_chapters                  WHERE batch_id=?1 AND chapter_id=?2",
                rusqlite::params![batch_id, chapter_id],
                |r| r.get(0),
            ).optional()?.ok_or_else(|| Error::NotFound(format!(
                "transformation_chapter not found for batch={batch_id} chapter={chapter_id}"
            )))?;
            let wr_id: i64 = tx.query_row(
                "SELECT id FROM workflow_results WHERE batch_id=?1",
                rusqlite::params![batch_id],
                |r| r.get(0),
            ).optional()?.ok_or_else(|| Error::NotFound(format!(
                "workflow_result not found for batch={batch_id}"
            )))?;
            let updated = tx.execute(
                "UPDATE workflow_result_chapters                  SET content=?3, updated_at=?4                  WHERE workflow_result_id=?1 AND chapter_id=?2",
                rusqlite::params![wr_id, chapter_id, draft_content, now],
            )?;
            if updated == 0 {
                return Err(Error::NotFound("workflow_result_chapter slot missing".into()));
            }
            tx.execute(
                "UPDATE transformation_chapters                  SET status='done', error=NULL, result_content=NULL, completed_at=?2                  WHERE id=?1",
                rusqlite::params![tc_id, now],
            )?;
            let mut tokens: (Option<i32>, Option<i32>) = (None, None);
            let mut source_used = false;
            if let Some(pid) = source_preview_id {
                let mut stmt = tx.prepare(
                    "SELECT tokens_in, tokens_out FROM chapter_previews WHERE id=?1",
                )?;
                let mut rows = stmt.query(rusqlite::params![pid])?;
                if let Some(row) = rows.next()? {
                    tokens = (row.get(0)?, row.get(1)?);
                    source_used = true;
                }
            }
            if !source_used {
                let mut stmt = tx.prepare(
                    "SELECT tokens_in, tokens_out FROM chapter_previews                  WHERE batch_id=?1 AND chapter_id=?2 AND status='done'                  ORDER BY id DESC LIMIT 1",
                )?;
                let mut rows = stmt.query(rusqlite::params![batch_id, chapter_id])?;
                if let Some(row) = rows.next()? {
                    tokens = (row.get(0)?, row.get(1)?);
                }
            }
            tx.execute(
                "UPDATE transformation_chapters                  SET tokens_in=?2, tokens_out=?3                  WHERE id=?1",
                rusqlite::params![tc_id, tokens.0, tokens.1],
            )?;
            tx.execute(
                "DELETE FROM chapter_previews WHERE batch_id=?1 AND chapter_id=?2",
                rusqlite::params![batch_id, chapter_id],
            )?;
            tx.commit()?;
        }
        db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))
    }
}

/// frontier 章节 id(spec §5.3):仅读当前工作流结果集里的最近非空 slot。
/// 跨工作流读取被禁止;失败/跳过的 slot 不计入。
fn frontier_chapter_id_in_workflow(
    conn: &rusqlite::Connection,
    batch_id: i64,
    chapter_id: i64,
) -> Result<Option<i64>> {
    let mut stmt = conn.prepare(
        "SELECT c.id FROM workflow_result_chapters wrc          JOIN workflow_results wr ON wr.id = wrc.workflow_result_id          JOIN chapters c ON c.id = wrc.chapter_id          WHERE wr.batch_id = ?1            AND wrc.content IS NOT NULL            AND c.idx < (SELECT idx FROM chapters WHERE id = ?2)          ORDER BY c.idx DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(rusqlite::params![batch_id, chapter_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

fn policy_str(p: OnFailurePolicy) -> &'static str {
    match p {
        OnFailurePolicy::PauseAndReview => "pause_and_review",
        OnFailurePolicy::Terminate => "terminate",
        OnFailurePolicy::SkipFailed => "skip_failed",
    }
}

fn mode_str(m: PromptKind) -> &'static str {
    match m {
        PromptKind::Compress => "compress",
        PromptKind::Style => "style",
    }
}
