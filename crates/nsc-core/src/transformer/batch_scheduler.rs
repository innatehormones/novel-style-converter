//! 批号调度器:按 frontier 串行派发,跨工作流不共享结果。
//!
//! 单例;持 `db_path`(不在 Db 上 Sync);由 lib.rs 在 JobQueue::set_notifier 时注册。
//!
//! 本片接:
//! - `create_workflow` 原子事务:batch + workflow_results + N 个 tc + N 个空 slot
//! - `on_chapter_done` / `on_chapter_failed` 派下一章(失败固定继续,不分支)
//! - 完成判据 → batch 状态迁移到 Running/Stopped 两态之一
//! - `safe_stop_on_dispatch_failure` dispatch 失败的兜底
//!
//! Task 7 已加:`stop_workflow` 人工停止 + `retry_empty_slots` 重试空槽。

use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;

use crate::db::Db;
use crate::error::{Error, Result};
use crate::models::{
    Batch, BatchStatus, Chapter, ModelConfig, NewBatch, OnFailurePolicy, Prompt,
    PromptKind, ResumeAction, TransformationNovel,
};
use crate::transformer::{JobQueue, JobSpec};

pub struct BatchScheduler {
    db_path: PathBuf,
    job_queue: Arc<JobQueue>,
}

/// Per-batch 可选覆盖：`create_batch` 时 prompt / model / mode / ctx 字段
/// 任一填了 None 就回退到 TN 默认；都给 None 等价于"用 TN 默认"。
#[derive(Debug, Default, Clone)]
pub struct BatchOverrides {
    pub prompt_id: Option<i64>,
    pub model_config_id: Option<i64>,
    pub mode: Option<PromptKind>,
    pub ctx_prev_original: Option<i32>,
    pub ctx_prev_transformed: Option<i32>,
    pub ctx_next_original: Option<i32>,
}

/// `create_workflow` 入参 —— 不走 TN 默认覆盖,字段全是必填。
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
}

impl BatchScheduler {
    pub fn new(db_path: PathBuf, job_queue: Arc<JobQueue>) -> Self {
        Self { db_path, job_queue }
    }

    /// 创建批号 + 立即派首章（其他章节等 JobQueue 完成回调再派）。
    /// 整批写入一个事务（batch 行 + N 个 tc 行）；dispatch 部分是 tx 外。
    /// `overrides` 给 None 时回退到 TN 默认；都给 None 时等价于"用 TN 默认"。
    pub fn create_batch(
        &self,
        new_batch: NewBatch,
        chapter_ids: Vec<i64>,
        overrides: BatchOverrides,
    ) -> Result<Batch> {
        let db = Db::open(&self.db_path)?;
        let tn_id = new_batch.transformation_novel_id;

        // 取 TN 的默认配置（必填：spec §4.4 兼容性策略）
        let tn = db.transformation_novels().get(tn_id)?
            .ok_or_else(|| Error::NotFound(format!("tn {tn_id} 不存在")))?;
        let prompt_id = overrides.prompt_id
            .or(tn.default_prompt_id)
            .ok_or_else(|| Error::NotFound("default_prompt 缺失".into()))?;
        let model_cfg_id = overrides.model_config_id
            .or(tn.default_model_config_id)
            .ok_or_else(|| Error::NotFound("default_model_config 缺失".into()))?;
        let mode = overrides.mode
            .or(tn.default_mode)
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

            // INSERT N × transformation_chapters（legacy 路径:style_ref_chapter_id = NULL,
            // 跨工作流读取结果已由 spec §5.3 禁止,老 create_batch/dispatch_batch 路径保留兼容行为）
            let mut ids = Vec::with_capacity(chapter_ids.len());
            for cid in &chapter_ids {
                tx.execute(
                    "INSERT INTO transformation_chapters \
                     (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
                      ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
                      batch_id, style_ref_chapter_id, status) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, 'pending')",
                    rusqlite::params![
                        tn_id,
                        *cid,
                        mode_str(mode),
                        prompt_id,
                        model_cfg_id,
                        overrides.ctx_prev_original.unwrap_or(0),
                        overrides.ctx_prev_transformed.unwrap_or(0),
                        overrides.ctx_next_original.unwrap_or(0),
                        batch_id,
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
        self.dispatch(
            &db, &tn, &prompt, &model, tids[0],
            overrides.ctx_prev_original.unwrap_or(0),
            overrides.ctx_prev_transformed.unwrap_or(0),
            overrides.ctx_next_original.unwrap_or(0),
        )?;

        // 读回 batch 实体
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound("batch 写入后回读失败".into()))?;
        Ok(batch)
    }

    /// 原子创建工作流（spec §5.1）：单事务里写 batches(status='running') +
    /// workflow_results + N × transformation_chapters(status='pending') +
    /// N × 空 workflow_result_chapters；事务外派首章。
    /// 字段全是必填,不回退 TN 默认 —— TN 默认覆盖在 CreateBatchDialog
    /// 已经收敛到具体值(spec 字段),这里只校验合法性与一致性。
    pub fn create_workflow(&self, spec: WorkflowCreate) -> Result<Batch> {
        if spec.chapter_ids.is_empty() {
            return Err(Error::Validation("必须选择至少一个章节".into()));
        }
        let db = Db::open(&self.db_path)?;

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
        let model = db.model_configs().get(spec.model_config_id)?
            .ok_or_else(|| Error::NotFound(format!("model_config {} 不存在", spec.model_config_id)))?;
        if PromptKind::from(prompt.kind) != spec.mode {
            return Err(Error::Validation("prompt kind 与 mode 不一致".into()));
        }

        // 2. 单事务：batch + 结果集 + N × tc + N × 空槽
        let now = Utc::now().to_rfc3339();
        let (batch_id, first_tid) = {
            let tx = db.conn.unchecked_transaction()?;
            tx.execute(
                "INSERT INTO batches (transformation_novel_id, label, on_failure_policy, status, created_at) \
                 VALUES (?1, ?2, 'pause_and_review', 'running', ?3)",
                rusqlite::params![spec.transformation_novel_id, spec.label, now],
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
            let mut first_tid: Option<i64> = None;
            for cid in &spec.chapter_ids {
                let frontier_cid = frontier_chapter_id_in_workflow(&tx, batch_id, *cid)?;
                tx.execute(
                    "INSERT INTO transformation_chapters \
                     (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
                      ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
                      batch_id, style_ref_chapter_id, status) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending')",
                    rusqlite::params![
                        spec.transformation_novel_id, *cid, mode_str(spec.mode),
                        spec.prompt_id, spec.model_config_id,
                        spec.ctx_prev_original, spec.ctx_prev_transformed, spec.ctx_next_original,
                        batch_id,
                        frontier_cid,
                    ],
                )?;
                let tid = tx.last_insert_rowid();
                if first_tid.is_none() { first_tid = Some(tid); }
                tx.execute(
                    "INSERT INTO workflow_result_chapters \
                     (workflow_result_id, chapter_id, content, created_at, updated_at) \
                     VALUES (?1, ?2, NULL, ?3, ?3)",
                    rusqlite::params![result_id, cid, now],
                )?;
            }
            tx.execute(
                "UPDATE batches SET started_at = ?1 WHERE id = ?2",
                rusqlite::params![now, batch_id],
            )?;
            tx.commit()?;
            (batch_id, first_tid.expect("chapter_ids 非空已校验"))
        };

        // 3. 派首章（事务外）
        let dispatch_res = self.dispatch(
            &db, &tn, &prompt, &model, first_tid,
            spec.ctx_prev_original, spec.ctx_prev_transformed, spec.ctx_next_original,
        );
        if let Err(e) = dispatch_res {
            // 兜底 safe_stop：原 dispatch 错误回给调用方,batch 内部转 Stopped。
            self.safe_stop_on_dispatch_failure(batch_id, first_tid, &e.to_string())?;
            return Err(e);
        }

        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound("batch 写入后回读失败".into()))?;
        Ok(batch)
    }

    /// `create_workflow` dispatch 失败的兜底：首章标 failed(若 worker 已置 running 也要拉回),
    /// 同 batch 其他 pending → skipped,batch → stopped 带 ended_at。
    fn safe_stop_on_dispatch_failure(&self, batch_id: i64, first_tid: i64, msg: &str) -> Result<()> {
        let db = Db::open(&self.db_path)?;
        let now = Utc::now().to_rfc3339();
        let tx = db.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE transformation_chapters SET status='failed', error=?2, completed_at=?3 \
             WHERE id=?1",
            rusqlite::params![first_tid, msg, now],
        )?;
        tx.execute(
            "UPDATE transformation_chapters SET status='skipped', completed_at=?2 \
             WHERE batch_id=?1 AND status='pending'",
            rusqlite::params![batch_id, now],
        )?;
        tx.execute(
            "UPDATE batches SET status='stopped', ended_at=?1 WHERE id=?2",
            rusqlite::params![now, batch_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 派发一个已有的 Pending batch：自动取 TN 全量章节 → 落 tc 行 → 派首章。
    /// batch 必须处于 Pending（已 dispatch 的 batch 不能再次派）。
    /// overrides 任意字段为 None 时回退到 TN 默认。
    pub fn dispatch_batch(
        &self,
        batch_id: i64,
        overrides: BatchOverrides,
    ) -> Result<Batch> {
        let db = Db::open(&self.db_path)?;
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        if !matches!(batch.status, BatchStatus::Pending) {
            return Err(Error::Validation(format!(
                "batch {batch_id} 不是 Pending（当前 {:?}），不能 dispatch",
                batch.status
            )));
        }
        let tn = db.transformation_novels().get(batch.transformation_novel_id)?
            .ok_or_else(|| Error::NotFound(format!(
                "tn {} 不存在", batch.transformation_novel_id
            )))?;
        let chapter_ids: Vec<i64> = db.chapters()
            .list_by_data_asset(tn.data_asset_id)?
            .into_iter()
            .map(|c| c.id)
            .collect();

        let prompt_id = overrides.prompt_id
            .or(tn.default_prompt_id)
            .ok_or_else(|| Error::NotFound("default_prompt 缺失".into()))?;
        let model_cfg_id = overrides.model_config_id
            .or(tn.default_model_config_id)
            .ok_or_else(|| Error::NotFound("default_model_config 缺失".into()))?;
        let mode = overrides.mode
            .or(tn.default_mode)
            .ok_or_else(|| Error::NotFound("default_mode 缺失".into()))?;
        let prompt = db.prompts().get(prompt_id)?
            .ok_or_else(|| Error::NotFound(format!("prompt {prompt_id} 不存在")))?;
        let model = db.model_configs().get(model_cfg_id)?
            .ok_or_else(|| Error::NotFound(format!("model_config {model_cfg_id} 不存在")))?;

        let now = Utc::now().to_rfc3339();
        let tids: Vec<i64>;
        {
            let tx = db.conn.unchecked_transaction()?;
            let mut ids = Vec::with_capacity(chapter_ids.len());
            for cid in &chapter_ids {
                tx.execute(
                    "INSERT INTO transformation_chapters \
                     (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
                      ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
                      batch_id, style_ref_chapter_id, status) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, 'pending')",
                    rusqlite::params![
                        batch.transformation_novel_id,
                        *cid,
                        mode_str(mode),
                        prompt_id,
                        model_cfg_id,
                        overrides.ctx_prev_original.unwrap_or(0),
                        overrides.ctx_prev_transformed.unwrap_or(0),
                        overrides.ctx_next_original.unwrap_or(0),
                        batch_id,
                    ],
                )?;
                ids.push(tx.last_insert_rowid());
            }
            tx.execute(
                "UPDATE batches SET status='running', started_at=?1 WHERE id=?2",
                rusqlite::params![now, batch_id],
            )?;
            tx.commit()?;
            tids = ids;
        }

        self.dispatch(
            &db, &tn, &prompt, &model, tids[0],
            overrides.ctx_prev_original.unwrap_or(0),
            overrides.ctx_prev_transformed.unwrap_or(0),
            overrides.ctx_next_original.unwrap_or(0),
        )?;

        let updated = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound("batch 回读失败".into()))?;
        Ok(updated)
    }

    /// 派发一个具体 tc（按 tid）。从 Db 读 chapter + frontier 章节 id，
    /// 构造 JobSpec 塞进 JobQueue。
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
            transformation_id: tid,
            // tc.mode 由 `create_workflow` / `create_batch` / `dispatch_batch`
            // 在 tc 行 INSERT 时写入(`mode_str(spec.mode)`),是 per-task 的权威值;
            // TN 默认覆盖已由 caller 用 `BatchOverrides::default().mode.unwrap_or(tn.default_mode)`
            // 收敛到具体值,这里再回退会双重叠加。
            mode: tc.mode,
            chapter: Chapter {
                id: chapter.id,
                data_asset_id: chapter.data_asset_id,
                idx: chapter.idx,
                title: chapter.title.clone(),
                body: chapter.body.clone(),
                word_count: chapter.word_count,
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

    /// JobQueue 完成回调：把正文写入结果集 + 派下一章（若还有）。
    /// 单事务里标 tc done（清空 `tc.result_content` 回到结果槽），同步写
    /// `workflow_result_chapters.content`，然后 `advance_batch`。
    pub fn on_chapter_done(&self, tid: i64, content: String) -> Result<()> {
        let db = Db::open(&self.db_path)?;
        let tc = db.transformation_chapters().get(tid)?
            .ok_or_else(|| Error::NotFound(format!("tc {tid} 不存在")))?;
        let batch_id = match tc.batch_id {
            Some(b) => b,
            None => return Ok(()),  // 散点行（非 batch 入队）不归 scheduler 管
        };
        let now = Utc::now().to_rfc3339();
        {
            let tx = db.conn.unchecked_transaction()?;
            // tc 行：保留已由 worker 写入的 tokens_in/out，清空 result_content（spec §5.x 收口到结果集）。
            tx.execute(
                "UPDATE transformation_chapters \
                 SET result_content=NULL, completed_at=?1 \
                 WHERE id=?2",
                rusqlite::params![now, tid],
            )?;
            // 同步写结果槽 —— `WorkflowResultRepo::write_content_by_chapter` 通过
            // sub-select 找 workflow_results.id，对未建结果集 / 缺槽的 batch 静默 noop，
            // 让老 batch（非工作流）路径也能调到这里而不报错。
            tx.execute(
                "UPDATE workflow_result_chapters \
                 SET content=?2, updated_at=?3 \
                 WHERE workflow_result_id = (SELECT id FROM workflow_results WHERE batch_id=?4) \
                   AND chapter_id=?1",
                rusqlite::params![tc.chapter_id, content, now, batch_id],
            )?;
            tx.commit()?;
        }
        self.advance_batch(&db, batch_id)
    }

    /// 失败回调:标 failed + 清空 result_content/tokens,再 advance_batch 派下一章。
    /// 不再按 on_failure_policy 分流(spec §3.3 收敛到单一行为)。
    /// batch 收尾交给 advance_batch → maybe_finalize_batch。
    pub fn on_chapter_failed(&self, tid: i64, error: String) -> Result<()> {
        let db = Db::open(&self.db_path)?;
        let tc = db.transformation_chapters().get(tid)?
            .ok_or_else(|| Error::NotFound(format!("tc {tid} 不存在")))?;
        let Some(batch_id) = tc.batch_id else { return Ok(()); };
        let now = Utc::now().to_rfc3339();
        {
            let tx = db.conn.unchecked_transaction()?;
            tx.execute(
                "UPDATE transformation_chapters \
                 SET status='failed', error=?2, completed_at=?3, result_content=NULL, \
                     tokens_in=NULL, tokens_out=NULL \
                 WHERE id=?1",
                rusqlite::params![tid, error, now],
            )?;
            tx.commit()?;
        }
        self.advance_batch(&db, batch_id)
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
            // 还有 pending → 派下一章。prompt_id / model_config_id 从 tc 行直接读,
            // 跟 create_workflow 派首章对齐:WorkflowCreate.prompt_id/model_config_id
            // 在事务里已经写进每个 tc 行(`INSERT ... prompt_id, model_config_id`),
            // 不再回退 tn.default_*(TN 默认可能是 null,而 workflow 显式提供了值)。
            // 不读 TN 默认这条路径本来会让"用户没填 TN 默认 + workflow 显式选 prompt"
            // 的合法组合 advance 时 NotFound,工作流卡在第一个 done 不再派下一章。
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
            "SELECT COUNT(*) FROM transformation_chapters \
             WHERE batch_id = ?1 AND status IN ('pending','running')",
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
                self.dispatch(&db, &tn, &prompt, &model, ch_id, 0, 0, 0)?;
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
            "UPDATE transformation_chapters SET status='skipped', completed_at=?2 \
             WHERE batch_id=?1 AND status='pending'",
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
        if !matches!(batch.status, BatchStatus::Stopped) {
            return Err(Error::Validation("只有 Stopped 工作流可重试".into()));
        }
        if chapter_ids.is_empty() {
            return Err(Error::Validation("必须至少选择一个章节".into()));
        }
        let first_tid: i64 = {
            let tx = db.conn.unchecked_transaction()?;
            for cid in chapter_ids {
                let updated = tx.execute(
                    "UPDATE transformation_chapters \
                     SET status='pending', error=NULL, result_content=NULL, \
                         tokens_in=NULL, tokens_out=NULL, started_at=NULL, completed_at=NULL \
                     WHERE batch_id=?1 \
                       AND chapter_id=?2 \
                       AND status IN ('failed','skipped') \
                       AND (SELECT content FROM workflow_result_chapters wrc \
                             JOIN workflow_results wr ON wr.id = wrc.workflow_result_id \
                             WHERE wr.batch_id = transformation_chapters.batch_id \
                               AND wrc.chapter_id = transformation_chapters.chapter_id) IS NULL",
                    rusqlite::params![batch_id, cid],
                )?;
                if updated == 0 {
                    return Err(Error::Validation(format!(
                        "章节 {cid} 不是可重试空槽(不存在/非 failed-skipped/结果槽非空)"
                    )));
                }
            }
            tx.execute(
                "UPDATE batches SET status='running', ended_at=NULL WHERE id=?1",
                rusqlite::params![batch_id],
            )?;
            let first_tid: i64 = tx.query_row(
                "SELECT tc.id FROM transformation_chapters tc \
                 JOIN chapters c ON c.id = tc.chapter_id \
                 WHERE tc.batch_id=?1 AND tc.status='pending' \
                 ORDER BY c.idx ASC LIMIT 1",
                rusqlite::params![batch_id],
                |r| r.get(0),
            )?;
            tx.commit()?;
            first_tid
        };
        // 派首章(事务外):仍然走 batch 上固化的 prompt/model,而不是回退到 TN 默认
        // (TN 默认覆盖已在 CreateBatchDialog 收敛,这里与 create_workflow 行为一致)。
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

    /// 测试用：当前 batch 状态（方便断言）。
    pub fn batch_status(&self, batch_id: i64) -> Result<BatchStatus> {
        let db = Db::open(&self.db_path)?;
        let b = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        Ok(b.status)
    }
}

/// frontier 章节 id（spec §5.3）：仅读当前工作流结果集里的最近非空 slot。
/// 跨工作流读取被禁止;失败/跳过的 slot 不计入。
fn frontier_chapter_id_in_workflow(
    conn: &rusqlite::Connection,
    batch_id: i64,
    chapter_id: i64,
) -> Result<Option<i64>> {
    let mut stmt = conn.prepare(
        "SELECT c.id FROM workflow_result_chapters wrc \
         JOIN workflow_results wr ON wr.id = wrc.workflow_result_id \
         JOIN chapters c ON c.id = wrc.chapter_id \
         WHERE wr.batch_id = ?1 \
           AND wrc.content IS NOT NULL \
           AND c.idx < (SELECT idx FROM chapters WHERE id = ?2) \
         ORDER BY c.idx DESC LIMIT 1",
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

