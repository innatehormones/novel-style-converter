//! 批号调度器：按 frontier（context inheritance）串行派发，跨 batch 取前序 done。
//!
//! 单例；持 `db_path`（不在 Db 上 Sync）；由 lib.rs 在 JobQueue::set_notifier 时注册。
//!
//! 本片只接：
//! - `create_batch` 写 batch + tc 行 + 算 frontier + 派首章
//! - `on_chapter_done` / `on_chapter_failed` 派下一章（SkipFailed 不接 → Slice 5）
//! - 完成判据 → batch 状态迁移
//!
//! Slice 5 再加：
//! - `on_failure_policy` 三分支
//! - `TransformStatus::Skipped`
//! - `resume(batch_id, action)`

use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;

use crate::db::Db;
use crate::error::{Error, Result};
use crate::models::{
    Batch, BatchStatus, Chapter, ModelConfig, NewBatch, OnFailurePolicy, Prompt,
    ResumeAction, TransformMode, TransformationNovel,
};
use crate::transformer::{JobQueue, JobSpec};

pub struct BatchScheduler {
    db_path: PathBuf,
    job_queue: Arc<JobQueue>,
}

impl BatchScheduler {
    pub fn new(db_path: PathBuf, job_queue: Arc<JobQueue>) -> Self {
        Self { db_path, job_queue }
    }

    /// 创建批号 + 立即派首章（其他章节等 JobQueue 完成回调再派）。
    /// 整批写入一个事务（batch 行 + N 个 tc 行）；dispatch 部分是 tx 外。
    pub fn create_batch(
        &self,
        new_batch: NewBatch,
        chapter_ids: Vec<i64>,
    ) -> Result<Batch> {
        let db = Db::open(&self.db_path)?;
        let tn_id = new_batch.transformation_novel_id;

        // 取 TN 的默认配置（必填：spec §4.4 兼容性策略）
        let tn = db.transformation_novels().get(tn_id)?
            .ok_or_else(|| Error::NotFound(format!("tn {tn_id} 不存在")))?;
        let prompt_id = tn.default_prompt_id
            .ok_or_else(|| Error::NotFound("default_prompt 缺失".into()))?;
        let model_cfg_id = tn.default_model_config_id
            .ok_or_else(|| Error::NotFound("default_model_config 缺失".into()))?;
        let mode = tn.default_mode
            .ok_or_else(|| Error::NotFound("default_mode 缺失".into()))?;
        let prompt = db.prompts().get(prompt_id)?
            .ok_or_else(|| Error::NotFound(format!("prompt {prompt_id} 不存在")))?;
        let model = db.model_configs().get(model_cfg_id)?
            .ok_or_else(|| Error::NotFound(format!("model_config {model_cfg_id} 不存在")))?;

        let now = Utc::now().to_rfc3339();
        let batch_id: i64;
        let tids: Vec<i64>;
        {
            let tx = db.conn.unchecked_transaction()?;

            // INSERT batches
            tx.execute(
                "INSERT INTO batches (transformation_novel_id, label, on_failure_policy, status, created_at) \
                 VALUES (?1, ?2, ?3, 'pending', ?4)",
                rusqlite::params![
                    tn_id,
                    new_batch.label.as_deref(),
                    policy_str(new_batch.on_failure_policy),
                    now,
                ],
            )?;
            batch_id = tx.last_insert_rowid();

            // INSERT N × transformation_chapters（带 frontier 算的 style_ref_chapter_id）
            let mut ids = Vec::with_capacity(chapter_ids.len());
            for cid in &chapter_ids {
                let frontier_cid = frontier_chapter_id(&tx, tn_id, *cid)?;
                tx.execute(
                    "INSERT INTO transformation_chapters \
                     (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
                      ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
                      batch_id, style_ref_chapter_id, status) \
                     VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 0, ?6, ?7, 'pending')",
                    rusqlite::params![
                        tn_id,
                        *cid,
                        mode_str(mode),
                        prompt_id,
                        model_cfg_id,
                        batch_id,
                        frontier_cid,
                    ],
                )?;
                ids.push(tx.last_insert_rowid());
            }
            // batch → running
            tx.execute(
                "UPDATE batches SET status='running', started_at=?1 WHERE id=?2",
                rusqlite::params![now, batch_id],
            )?;
            tx.commit()?;
            tids = ids;
        }

        // 派首章
        self.dispatch(&db, &tn, &prompt, &model, tids[0])?;

        // 读回 batch 实体
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound("batch 写入后回读失败".into()))?;
        Ok(batch)
    }

    /// 派发一个具体 tc（按 tid）。从 Db 读 chapter + frontier 章节 id，
    /// 构造 JobSpec 塞进 JobQueue。
    pub(crate) fn dispatch(
        &self,
        db: &Db,
        tn: &TransformationNovel,
        prompt: &Prompt,
        model: &ModelConfig,
        tid: i64,
    ) -> Result<()> {
        let tc = db.transformation_chapters().get(tid)?
            .ok_or_else(|| Error::NotFound(format!("tc {tid} 不存在")))?;
        let chapter = db.chapters().get(tc.chapter_id)?
            .ok_or_else(|| Error::NotFound(format!("chapter {} 不存在", tc.chapter_id)))?;

        let spec = JobSpec {
            transformation_id: tid,
            mode: tn.default_mode.unwrap_or(tc.mode),
            chapter: Chapter {
                id: chapter.id,
                data_asset_id: chapter.data_asset_id,
                idx: chapter.idx,
                title: chapter.title.clone(),
                byte_start: chapter.byte_start,
                byte_end: chapter.byte_end,
                word_count: chapter.word_count,
            },
            prompt: prompt.clone(),
            model_config: model.clone(),
            ctx_prev_original: 0,
            ctx_prev_transformed: 0,
            ctx_next_original: 0,
        };
        self.job_queue.enqueue(spec);
        Ok(())
    }

    /// JobQueue 完成回调：派发 batch 内的下一章（若还有）。
    pub fn on_chapter_done(&self, tid: i64) -> Result<()> {
        let db = Db::open(&self.db_path)?;
        let tc = db.transformation_chapters().get(tid)?
            .ok_or_else(|| Error::NotFound(format!("tc {tid} 不存在")))?;
        let batch_id = match tc.batch_id {
            Some(b) => b,
            None => return Ok(()),  // 散点行（非 batch 入队）不归 scheduler 管
        };
        self.advance_batch(&db, batch_id)
    }

    /// 失败回调：占位实现 —— Slice 5 才接 policy 分流。
    /// 本片只保证不 panic、不重复 dispatch。
    pub fn on_chapter_failed(&self, tid: i64, error: String) -> Result<()> {
        let db = Db::open(&self.db_path)?;
        let tc = db.transformation_chapters().get(tid)?
            .ok_or_else(|| Error::NotFound(format!("tc {tid} 不存在")))?;
        let batch_id = match tc.batch_id {
            Some(b) => b,
            None => return Ok(()),
        };
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;

        let now = Utc::now().to_rfc3339();
        let tx = db.conn.unchecked_transaction()?;
        match batch.on_failure_policy {
            OnFailurePolicy::PauseAndReview => {
                // tc 已是 failed（JobQueue worker 在 mark_failed 时写了）。
                tx.execute(
                    "UPDATE batches SET status='paused' WHERE id=?1",
                    rusqlite::params![batch_id],
                )?;
            }
            OnFailurePolicy::Terminate => {
                // 同 batch 内所有 pending → cancelled
                tx.execute(
                    "UPDATE transformation_chapters SET status='cancelled' \
                     WHERE batch_id=?1 AND status='pending'",
                    rusqlite::params![batch_id],
                )?;
                tx.execute(
                    "UPDATE batches SET status='terminated', ended_at=?1 WHERE id=?2",
                    rusqlite::params![now, batch_id],
                )?;
            }
            OnFailurePolicy::SkipFailed => {
                // 把这一章标 skipped（保留 error）
                tx.execute(
                    "UPDATE transformation_chapters SET status='skipped', error=?2, \
                        result_content=NULL, tokens_in=NULL, tokens_out=NULL, completed_at=?3 \
                     WHERE id=?1",
                    rusqlite::params![tid, &error, &now],
                )?;
                // 不改 batch.status；继续 dispatch（在 commit 之后做）
            }
        }
        tx.commit()?;

        if matches!(batch.on_failure_policy, OnFailurePolicy::SkipFailed) {
            // 派下一章
            return self.advance_batch(&db, batch_id);
        }
        Ok(())
    }

    /// 派下一章（若有）；完成判据。
    fn advance_batch(&self, db: &Db, batch_id: i64) -> Result<()> {
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;

        // 取 batch 内第一个 pending 行（按 chapter_idx ASC）
        let next_tid: Option<i64> = {
            let mut stmt = db.conn.prepare(
                "SELECT transformation_chapters.id FROM transformation_chapters \
                 JOIN chapters c ON c.id = transformation_chapters.chapter_id \
                 WHERE transformation_chapters.batch_id = ?1 \
                   AND transformation_chapters.status = 'pending' \
                 ORDER BY c.idx ASC, transformation_chapters.id ASC \
                 LIMIT 1",
            )?;
            let mut rows = stmt.query(rusqlite::params![batch_id])?;
            if let Some(row) = rows.next()? { Some(row.get(0)?) } else { None }
        };

        if let Some(tid) = next_tid {
            // 还有 pending → 取 TN + prompt + model 派发
            let tn_id = batch.transformation_novel_id;
            let tn = db.transformation_novels().get(tn_id)?
                .ok_or_else(|| Error::NotFound(format!("tn {tn_id} 不存在")))?;
            let prompt_id = tn.default_prompt_id
                .ok_or_else(|| Error::NotFound("default_prompt 缺失".into()))?;
            let model_cfg_id = tn.default_model_config_id
                .ok_or_else(|| Error::NotFound("default_model_config 缺失".into()))?;
            let prompt = db.prompts().get(prompt_id)?
                .ok_or_else(|| Error::NotFound(format!("prompt {prompt_id} 不存在")))?;
            let model = db.model_configs().get(model_cfg_id)?
                .ok_or_else(|| Error::NotFound(format!("model_config {model_cfg_id} 不存在")))?;
            return self.dispatch(db, &tn, &prompt, &model, tid);
        }

        // 没 pending 了 → 完成判据
        self.maybe_finalize_batch(db, batch_id)
    }

    /// §5.6.1 完成判据：
    /// - completed 当且仅当 批次内不存在 pending/running/failed 且至少一行 done
    /// - terminated 当且仅当 批次内不存在 pending/running/failed 且全无 done
    fn maybe_finalize_batch(&self, db: &Db, batch_id: i64) -> Result<()> {
        let active_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM transformation_chapters \
             WHERE batch_id = ?1 AND status IN ('pending','running','failed')",
            rusqlite::params![batch_id],
            |row| row.get(0),
        )?;
        if active_count > 0 {
            return Ok(());  // 还有 pending/running/failed，不动
        }
        let done_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM transformation_chapters \
             WHERE batch_id = ?1 AND status = 'done'",
            rusqlite::params![batch_id],
            |row| row.get(0),
        )?;
        let now = Utc::now().to_rfc3339();
        let new_status = if done_count > 0 { "completed" } else { "terminated" };
        db.conn.execute(
            "UPDATE batches SET status=?1, ended_at=?2 WHERE id=?3",
            rusqlite::params![new_status, now, batch_id],
        )?;
        Ok(())
    }

    /// 用户在 paused 时介入。三种动作：
    ///   Retry(ch_id):    tc 重置为 pending + 立即 dispatch（绕过 batch 头）
    ///   Skip(ch_id):     tc 标 skipped + dispatch 下一章
    ///   Terminate:       同 batch 后续 pending → cancelled, batch Terminated
    pub fn resume(&self, batch_id: i64, action: ResumeAction) -> Result<Batch> {
        let db = Db::open(&self.db_path)?;
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        if !matches!(batch.status, BatchStatus::Paused) {
            return Err(Error::Validation(format!(
                "batch {batch_id} 不是 Paused（当前 {:?}），不能 resume",
                batch.status
            )));
        }

        let now = Utc::now().to_rfc3339();
        let tx = db.conn.unchecked_transaction()?;
        match action {
            ResumeAction::Retry(ch_id) => {
                tx.execute(
                    "UPDATE transformation_chapters \
                     SET status='pending', result_content=NULL, tokens_in=NULL, tokens_out=NULL, \
                         error=NULL, started_at=NULL, completed_at=NULL \
                     WHERE id=?1 AND batch_id=?2",
                    rusqlite::params![ch_id, batch_id],
                )?;
                tx.execute(
                    "UPDATE batches SET status='running', ended_at=NULL WHERE id=?1",
                    rusqlite::params![batch_id],
                )?;
                tx.commit()?;

                // 立即 dispatch this ch
                let tn_id = batch.transformation_novel_id;
                let tn = db.transformation_novels().get(tn_id)?
                    .ok_or_else(|| Error::NotFound(format!("tn {tn_id} 不存在")))?;
                let prompt_id = tn.default_prompt_id
                    .ok_or_else(|| Error::NotFound("default_prompt 缺失".into()))?;
                let model_cfg_id = tn.default_model_config_id
                    .ok_or_else(|| Error::NotFound("default_model_config 缺失".into()))?;
                let prompt = db.prompts().get(prompt_id)?
                    .ok_or_else(|| Error::NotFound(format!("prompt {prompt_id} 不存在")))?;
                let model = db.model_configs().get(model_cfg_id)?
                    .ok_or_else(|| Error::NotFound(format!("model_config {model_cfg_id} 不存在")))?;
                self.dispatch(&db, &tn, &prompt, &model, ch_id)?;
            }
            ResumeAction::Skip(ch_id) => {
                tx.execute(
                    "UPDATE transformation_chapters SET status='skipped', completed_at=?2 \
                     WHERE id=?1 AND batch_id=?3",
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
                    "UPDATE transformation_chapters SET status='cancelled' \
                     WHERE batch_id=?1 AND status='pending'",
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

    /// 测试用：当前 batch 状态（方便断言）。
    pub fn batch_status(&self, batch_id: i64) -> Result<BatchStatus> {
        let db = Db::open(&self.db_path)?;
        let b = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        Ok(b.status)
    }
}

/// frontier 章节 id（spec §5.8）：
/// 跨 batch、跨 prompt/model 取同 tn 内 idx 严格小于当前章节、status='done' 的最近一次 tc。
/// 返回 None（首次转换 / 无前置）→ tc.style_ref_chapter_id = NULL。
fn frontier_chapter_id(
    conn: &rusqlite::Connection,
    tn_id: i64,
    chapter_id: i64,
) -> Result<Option<i64>> {
    let mut stmt = conn.prepare(
        "SELECT c.id FROM transformation_chapters tc \
         JOIN chapters c ON c.id = tc.chapter_id \
         WHERE tc.transformation_novel_id = ?1 \
           AND tc.status = 'done' \
           AND c.idx < (SELECT idx FROM chapters WHERE id = ?2) \
         ORDER BY c.idx DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(rusqlite::params![tn_id, chapter_id])?;
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

fn mode_str(m: TransformMode) -> &'static str {
    match m {
        TransformMode::Compress => "compress",
        TransformMode::Style => "style",
    }
}