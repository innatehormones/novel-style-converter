# Workflow Results 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把转换工程拆成“不可变蓝本 + 工作流 + 独立结果集”，让同一章节可在多个工作流中独立产生并查看结果，并加入人工停止、空槽重试与启动安全恢复。

**Architecture:** 复用现有 `batches` 表作为工作流容器，新增 `workflow_results` / `workflow_result_chapters` 两张结果表。调度核心下沉到 `BatchScheduler::create_workflow` 单一原子事务，并在事务外仅派章节序号最小的任务；取消 `on_failure_policy` 多分支，失败固定标记为 `Failed` 后继续派下一章；上下文查询改为只读当前工作流结果集。

**Tech Stack:** Rust（rusqlite 事务、nsc-core repo + scheduler、Tauri 2 IPC）、Vue 3 + Pinia + vitest、SQLite migration 0011。

---

## 文件结构（变更前先定位）

- 新增：
  - `migrations/0011_workflow_results.sql`
  - `crates/nsc-core/src/db/repo/workflow_result.rs`
  - `crates/nsc-core/src/models/workflow_result.rs`
  - `src-tauri/src/commands/workflows.rs`（聚合新建 / 停止 / 重试 / 查询命令）
- 修改：
  - `crates/nsc-core/src/db/repo/mod.rs`
  - `crates/nsc-core/src/models/mod.rs`
  - `crates/nsc-core/src/transformer/batch_scheduler.rs`
  - `crates/nsc-core/src/transformer/mod.rs`
  - `src-tauri/src/commands/batches.rs`（仅保留旧 IPC wrapper + 兼容迁移）
  - `src-tauri/src/lib.rs`
  - `src/ipc/types.ts`
  - `src/ipc/commands.ts`
  - `src/stores/batches.ts` → 新增 `workflows.ts`
  - `src/views/TransformationNovelDetail.vue`
  - `src/components/CreateBatchDialog.vue`（移除失败策略字段，接收已选章节 ID）
- 删除（迁移完成后）：
  - `src-tauri/src/commands/batches.rs` 中的 `create_batch` / `dispatch_batch` / `resume_batch` 旧 IPC 函数

## 风险/约束

- `Db::open` 会被多次调用，migration 必须全部 `IF NOT EXISTS` / `INSERT OR IGNORE` 幂等。
- worker 不持有 `Arc<Db>`，需继续走 `db_path` reopen 模式。
- `JobQueue::set_notifier` 仍为旧 `on_chapter_done/failed`；调度层要在 callback 内更新结果槽。
- 旧 `BatchStatus::{Pending,Paused,Completed,Terminated,Cancelled}` 暂保留枚举兼容迁移期间 IPC；新流程仅写 `Running/Stopped`。

---

## Task 1: Migration 0011 — 结果表与回填

**Files:**
- Create: `migrations/0011_workflow_results.sql`
- Test: `crates/nsc-core/tests/migration_workflow_results.rs`

- [ ] **Step 1: 写失败测试**

```rust
// crates/nsc-core/tests/migration_workflow_results.rs
use nsc_core::db::Db;
use nsc_core::models::{BatchStatus, OnFailurePolicy, TransformStatus, TransformMode};

#[test]
fn migration_0011_creates_workflow_results_and_seeds_for_existing_batches() {
    let db = Db::open_in_memory().unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        title: "t".into(),
        data_asset_id: 1,
    }).unwrap();
    let batch_id = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: None,
        on_failure_policy: OnFailurePolicy::PauseAndReview,
    }).unwrap();
    db.batches().set_status(batch_id, BatchStatus::Running).unwrap();
    let tc_id = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id,
        chapter_id: 1,
        mode: TransformMode::Compress,
        prompt_id: 1,
        model_config_id: 1,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        batch_id: Some(batch_id),
        style_ref_chapter_id: None,
    }).unwrap();
    db.transformation_chapters().mark_done(tc_id, "result".into(), 1, 1).unwrap();
    // 模拟重新打开 DB: 重跑 migration 应不报错,并已建立对应结果集
    let db2 = Db::open_in_memory().unwrap();
    // 这里断言已经在 run_migrations 中触发;真正的可见性在步骤 3 才测
    drop(db2);
    // 通过 helper 直接验证结果表
    let result_id: i64 = db.conn.query_row(
        "SELECT id FROM workflow_results WHERE batch_id = ?1",
        rusqlite::params![batch_id], |r| r.get(0)
    ).expect("结果集应已建立");
    let content: Option<String> = db.conn.query_row(
        "SELECT content FROM workflow_result_chapters WHERE workflow_result_id = ?1",
        rusqlite::params![result_id], |r| r.get(0)
    ).unwrap();
    assert_eq!(content.as_deref(), Some("result"));
    assert_eq!(
        db.transformation_chapters().get(tc_id).unwrap().unwrap().status,
        TransformStatus::Done
    );
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test -p nsc-core --test migration_workflow_results -- --nocapture
```

Expected: FAIL（`workflow_results` 表不存在）。

- [ ] **Step 3: 写 migration**

```sql
-- migrations/0011_workflow_results.sql
CREATE TABLE IF NOT EXISTS workflow_results (
  id         INTEGER PRIMARY KEY,
  batch_id   INTEGER NOT NULL UNIQUE REFERENCES batches(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS workflow_result_chapters (
  id                 INTEGER PRIMARY KEY,
  workflow_result_id INTEGER NOT NULL REFERENCES workflow_results(id) ON DELETE CASCADE,
  chapter_id         INTEGER NOT NULL REFERENCES chapters(id),
  content            TEXT,
  created_at         TEXT NOT NULL,
  updated_at         TEXT NOT NULL,
  UNIQUE(workflow_result_id, chapter_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_tc_batch_chapter
  ON transformation_chapters(batch_id, chapter_id)
  WHERE batch_id IS NOT NULL;

-- 回填:为每个存量 batch 建立结果集(幂等)
INSERT OR IGNORE INTO workflow_results (id, batch_id, created_at)
SELECT id, id, created_at FROM batches;

-- 回填:为每条带 batch_id 的 task 建立空结果槽;Done 时回填旧 result_content
INSERT OR IGNORE INTO workflow_result_chapters
  (workflow_result_id, chapter_id, content, created_at, updated_at)
SELECT wr.id, tc.chapter_id,
       CASE WHEN tc.status='done' THEN tc.result_content ELSE NULL END,
       COALESCE(tc.completed_at, COALESCE(tc.started_at, wr.created_at)),
       COALESCE(tc.completed_at, COALESCE(tc.started_at, wr.created_at))
  FROM transformation_chapters tc
  JOIN workflow_results wr ON wr.batch_id = tc.batch_id
 WHERE tc.batch_id IS NOT NULL;

-- 任务状态:存量 cancelled → skipped
UPDATE transformation_chapters SET status='skipped'
 WHERE status='cancelled' AND batch_id IS NOT NULL;

-- 批量 batch 状态归档为 stopped(若尚未终态)
UPDATE batches
   SET status='stopped',
       ended_at = COALESCE(ended_at, started_at, created_at)
 WHERE status IN ('pending','running','paused','completed','terminated','cancelled');
```

并把 migration 加载顺序追加到 `crates/nsc-core/src/db/migrate.rs` 的 `SCHEMAS` 末尾。

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test -p nsc-core --test migration_workflow_results
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add migrations/0011_workflow_results.sql crates/nsc-core/src/db/migrate.rs crates/nsc-core/tests/migration_workflow_results.rs
git commit -m "feat(db): migration 0011 workflow results + backfill"
```

---

## Task 2: workflow_results 实体与 repo

**Files:**
- Create: `crates/nsc-core/src/models/workflow_result.rs`
- Create: `crates/nsc-core/src/db/repo/workflow_result.rs`
- Modify: `crates/nsc-core/src/models/mod.rs`
- Modify: `crates/nsc-core/src/db/repo/mod.rs`
- Test: `crates/nsc-core/tests/db_workflow_result.rs`

- [ ] **Step 1: 写失败测试**

```rust
// crates/nsc-core/tests/db_workflow_result.rs
use nsc_core::db::Db;

#[test]
fn create_result_chapters_for_batch_in_single_tx() {
    let db = Db::open_in_memory().unwrap();
    let batch_id: i64 = db.conn.query_row(
        "INSERT INTO batches (transformation_novel_id, label, on_failure_policy, status, created_at)
         VALUES (1, NULL, 'pause_and_review', 'running', '2026-08-04T00:00:00Z') RETURNING id",
        [], |r| r.get(0)
    ).unwrap();
    let result_id = db.workflow_results().create_for_batch(batch_id).unwrap();
    db.workflow_result_chapters().ensure_slots(result_id, &[1, 2, 3]).unwrap();
    let slots: Vec<Option<String>> = db.conn.prepare(
        "SELECT content FROM workflow_result_chapters WHERE workflow_result_id=?1 ORDER BY chapter_id"
    ).unwrap().query_map(rusqlite::params![result_id], |r| r.get(0)).unwrap()
     .collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(slots, vec![None, None, None]);
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test -p nsc-core --test db_workflow_result
```

Expected: FAIL（`workflow_results` 方法不存在）。

- [ ] **Step 3: 实现 model + repo**

`crates/nsc-core/src/models/workflow_result.rs`：

```rust
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct WorkflowResult {
    pub id: i64,
    pub batch_id: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct WorkflowResultChapter {
    pub id: i64,
    pub workflow_result_id: i64,
    pub chapter_id: i64,
    pub content: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

`crates/nsc-core/src/db/repo/workflow_result.rs`：

```rust
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::models::{WorkflowResult, WorkflowResultChapter};

pub struct WorkflowResultRepo<'a> { pub(crate) conn: &'a Connection }

impl<'a> WorkflowResultRepo<'a> {
    /// 在同一事务里创建结果集 + N 个空结果槽;事务失败则全部回滚。
    pub fn create_for_batch_with_slots(
        &self,
        batch_id: i64,
        chapter_ids: &[i64],
    ) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "INSERT OR IGNORE INTO workflow_results (batch_id, created_at) VALUES (?1, ?2)",
            params![batch_id, now],
        )?;
        let result_id: i64 = tx.query_row(
            "SELECT id FROM workflow_results WHERE batch_id = ?1",
            params![batch_id], |r| r.get(0),
        )?;
        for cid in chapter_ids {
            tx.execute(
                "INSERT OR IGNORE INTO workflow_result_chapters \
                 (workflow_result_id, chapter_id, content, created_at, updated_at) \
                 VALUES (?1, ?2, NULL, ?3, ?3)",
                params![result_id, cid, now],
            )?;
        }
        tx.commit()?;
        Ok(result_id)
    }

    pub fn get_by_batch(&self, batch_id: i64) -> Result<Option<WorkflowResult>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, batch_id, created_at FROM workflow_results WHERE batch_id = ?1"
        )?;
        let mut rows = stmt.query(params![batch_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_result(row)?))
        } else { Ok(None) }
    }

    pub fn list_chapters(&self, result_id: i64) -> Result<Vec<WorkflowResultChapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, workflow_result_id, chapter_id, content, created_at, updated_at \
             FROM workflow_result_chapters WHERE workflow_result_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![result_id], |r| row_to_chapter(r))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn write_content(&self, chapter_id: i64, content: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE workflow_result_chapters SET content = ?2, updated_at = ?3 WHERE id = ?1",
            params![chapter_id, content, now],
        )?;
        Ok(())
    }
}

fn row_to_result(row: &Row<'_>) -> rusqlite::Result<WorkflowResult> {
    let created: String = row.get(2)?;
    let dt = DateTime::parse_from_rfc3339(&created)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e)))?;
    Ok(WorkflowResult { id: row.get(0)?, batch_id: row.get(1)?, created_at: dt })
}

fn row_to_chapter(row: &Row<'_>) -> rusqlite::Result<WorkflowResultChapter> {
    let created: String = row.get(4)?;
    let updated: String = row.get(5)?;
    let parse = |s: String, idx: usize| DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e)));
    Ok(WorkflowResultChapter {
        id: row.get(0)?,
        workflow_result_id: row.get(1)?,
        chapter_id: row.get(2)?,
        content: row.get(3)?,
        created_at: parse(created, 4)?,
        updated_at: parse(updated, 5)?,
    })
}
```

- [ ] **Step 4: 导出到 `models/mod.rs` 与 `repo/mod.rs`**

```rust
// models/mod.rs
pub mod workflow_result;
pub use workflow_result::{WorkflowResult, WorkflowResultChapter};

// repo/mod.rs
pub mod workflow_result;
pub use workflow_result::WorkflowResultRepo;
```

`pool.rs` 加 `pub fn workflow_results(&self) -> WorkflowResultRepo<'_>`。

- [ ] **Step 5: 运行测试确认通过**

```bash
cargo test -p nsc-core --test db_workflow_result
```

Expected: PASS。

- [ ] **Step 6: 提交**

```bash
git add crates/nsc-core/src/models/workflow_result.rs crates/nsc-core/src/models/mod.rs \
        crates/nsc-core/src/db/repo/workflow_result.rs crates/nsc-core/src/db/repo/mod.rs \
        crates/nsc-core/src/db/pool.rs crates/nsc-core/tests/db_workflow_result.rs
git commit -m "feat(core): workflow result entities + repo"
```

---

## Task 3: BatchScheduler::create_workflow 原子创建 + 两态生命周期

**Files:**
- Modify: `crates/nsc-core/src/transformer/batch_scheduler.rs`
- Test: `crates/nsc-core/tests/scheduler.rs`（追加用例）

- [ ] **Step 1: 写失败测试**

```rust
// crates/nsc-core/tests/scheduler.rs（追加）
#[test]
fn create_workflow_is_atomic_and_initial_running() {
    let (sched, _db_path) = setup_scheduler(/* mock LLM */);
    let tn_id = seed_tn(&sched.db_path);
    let batch_id = sched.create_workflow(WorkflowCreate {
        transformation_novel_id: tn_id,
        label: Some("v1".into()),
        chapter_ids: vec![1, 2],
        prompt_id: 1,
        model_config_id: 1,
        mode: TransformMode::Compress,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    }).unwrap().id;
    assert_eq!(sched.batch_status(batch_id).unwrap(), BatchStatus::Running);
    let tc_count = sched.db.transformation_chapters().count_by_batch(batch_id).unwrap();
    assert_eq!(tc_count, 2);
    let slot_count: i64 = sched.db.conn.query_row(
        "SELECT COUNT(*) FROM workflow_result_chapters wrc
         JOIN workflow_results wr ON wr.id = wrc.workflow_result_id
         WHERE wr.batch_id = ?1", rusqlite::params![batch_id], |r| r.get(0)
    ).unwrap();
    assert_eq!(slot_count, 2);
}
```

`create_workflow` 与 `WorkflowCreate` 结构尚不存在，先观察测试不通过。

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test -p nsc-core --test scheduler
```

Expected: FAIL（`create_workflow` 未定义）。

- [ ] **Step 3: 实现 `WorkflowCreate` + `create_workflow`**

在 `batch_scheduler.rs` 中加：

```rust
#[derive(Debug, Clone)]
pub struct WorkflowCreate {
    pub transformation_novel_id: i64,
    pub label: Option<String>,
    pub chapter_ids: Vec<i64>,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub mode: TransformMode,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
}

impl BatchScheduler {
    /// spec §5.1:在一个事务内完成 batch + result set + tasks + slots,事务外派首章。
    pub fn create_workflow(&self, spec: WorkflowCreate) -> Result<Batch> {
        if spec.chapter_ids.is_empty() {
            return Err(Error::Validation("必须选择至少一个章节".into()));
        }
        let db = Db::open(&self.db_path)?;
        let tn = db.transformation_novels().get(spec.transformation_novel_id)?
            .ok_or_else(|| Error::NotFound(format!("tn {} 不存在", spec.transformation_novel_id)))?;
        // 校验章节归属
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
        if TransformMode::from(prompt.kind) != spec.mode {
            return Err(Error::Validation("prompt kind 与 mode 不一致".into()));
        }
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
                tx.execute(
                    "INSERT INTO transformation_chapters \
                     (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
                      ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
                      batch_id, style_ref_chapter_id, status) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, 'pending')",
                    rusqlite::params![
                        spec.transformation_novel_id, *cid, mode_str(spec.mode),
                        spec.prompt_id, spec.model_config_id,
                        spec.ctx_prev_original, spec.ctx_prev_transformed, spec.ctx_next_original,
                        batch_id,
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
            (batch_id, first_tid.expect("已校验非空"))
        };
        // 派首章(队列失败需安全停止)
        let dispatch_res = self.dispatch(&db, &tn, &prompt, &model, first_tid,
            spec.ctx_prev_original, spec.ctx_prev_transformed, spec.ctx_next_original);
        if let Err(e) = dispatch_res {
            self.safe_stop_on_dispatch_failure(batch_id, first_tid, &e.to_string())?;
            return Err(e);
        }
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound("batch 写入后回读失败".into()))?;
        Ok(batch)
    }

    fn safe_stop_on_dispatch_failure(&self, batch_id: i64, tid: i64, msg: &str) -> Result<()> {
        let db = Db::open(&self.db_path)?;
        let now = Utc::now().to_rfc3339();
        let tx = db.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE transformation_chapters SET status='failed', error=?2, completed_at=?3 \
             WHERE id=?1",
            rusqlite::params![tid, msg, now],
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
}
```

并在文件顶部 `use crate::models::TransformMode;` 与 `TransformMode::from(prompt.kind)` 已有映射。

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test -p nsc-core --test scheduler
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/nsc-core/src/transformer/batch_scheduler.rs crates/nsc-core/tests/scheduler.rs
git commit -m "feat(scheduler): atomic create_workflow with workflow result slots"
```

---

## Task 4: 失败继续 + 任务 / 批状态收敛到 Running/Stopped

**Files:**
- Modify: `crates/nsc-core/src/transformer/batch_scheduler.rs`
- Test: `crates/nsc-core/tests/scheduler.rs`

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn failed_chapter_is_marked_and_next_chapter_runs() {
    let (sched, _db) = setup_scheduler(/* LLM: fail first, succeed second */);
    let batch_id = sched.create_workflow(/* chapter_ids: [1,2] */).unwrap().id;
    wait_until_idle(&sched);
    let statuses: Vec<TransformStatus> = sched.db.transformation_chapters()
        .list_by_batch(batch_id).unwrap().iter().map(|t| t.status).collect();
    assert_eq!(statuses, vec![TransformStatus::Failed, TransformStatus::Done]);
    assert_eq!(sched.batch_status(batch_id).unwrap(), BatchStatus::Stopped);
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test -p nsc-core --test scheduler
```

Expected: FAIL（现状下 `paused` 不会停止）。

- [ ] **Step 3: 重写 `on_chapter_failed` 与 `maybe_finalize_batch`**

```rust
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
             SET status='failed', error=?2, completed_at=?3, result_content=NULL \
             WHERE id=?1",
            rusqlite::params![tid, error, now],
        )?;
        tx.commit()?;
    }
    self.advance_batch(&db, batch_id)
}

fn maybe_finalize_batch(&self, db: &Db, batch_id: i64) -> Result<()> {
    let active: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM transformation_chapters \
         WHERE batch_id = ?1 AND status IN ('pending','running','failed')",
        rusqlite::params![batch_id], |r| r.get(0),
    )?;
    if active > 0 { return Ok(()); }
    let now = Utc::now().to_rfc3339();
    db.conn.execute(
        "UPDATE batches SET status='stopped', ended_at = COALESCE(ended_at, ?1) WHERE id = ?2",
        rusqlite::params![now, batch_id],
    )?;
    Ok(())
}
```

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test -p nsc-core --test scheduler
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/nsc-core/src/transformer/batch_scheduler.rs crates/nsc-core/tests/scheduler.rs
git commit -m "feat(scheduler): failure marks failed and continues; finalize as stopped"
```

---

## Task 5: Worker 成功回调写结果槽

**Files:**
- Modify: `crates/nsc-core/src/transformer/queue.rs`（`mark_done` 路径改为不写 `result_content`，由 scheduler 在回调里写结果槽）
- Modify: `crates/nsc-core/src/transformer/batch_scheduler.rs`
- Test: `crates/nsc-core/tests/queue.rs`（追加）

- [ ] **Step 1: 写失败测试**

```rust
// crates/nsc-core/tests/queue.rs
#[test]
fn worker_success_writes_workflow_result_slot_not_tc_result_content() {
    let (sched, _db) = setup_scheduler(/* LLM: success */);
    let batch_id = sched.create_workflow(/* 1 chapter */).unwrap().id;
    wait_until_idle(&sched);
    let tc = sched.db.transformation_chapters().list_by_batch(batch_id).unwrap().remove(0);
    assert_eq!(tc.status, TransformStatus::Done);
    assert!(tc.result_content.is_none(), "tc.result_content 不再写");
    let slot_content: String = sched.db.conn.query_row(
        "SELECT content FROM workflow_result_chapters wrc \
         JOIN workflow_results wr ON wr.id = wrc.workflow_result_id \
         WHERE wr.batch_id = ?1",
        rusqlite::params![batch_id], |r| r.get(0)
    ).unwrap();
    assert!(!slot_content.is_empty());
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test -p nsc-core --test queue
```

Expected: FAIL（`tc.result_content` 仍被写）。

- [ ] **Step 3: 拆分回调**

`queue.rs` worker 成功路径改为：

```rust
// worker 成功只标 done + 落 tokens,content 留空
repo.mark_done(tid, String::new(), tokens_in, tokens_out)?;
```

`batch_scheduler.rs` 在 `on_chapter_done` 内由结果集找到 `chapter_id` 对应的 slot 并写入：

```rust
pub fn on_chapter_done(&self, tid: i64, content: String, tokens_in: i32, tokens_out: i32) -> Result<()> {
    let db = Db::open(&self.db_path)?;
    let tc = db.transformation_chapters().get(tid)?
        .ok_or_else(|| Error::NotFound(format!("tc {tid} 不存在")))?;
    let Some(batch_id) = tc.batch_id else { return Ok(()); };
    let now = Utc::now().to_rfc3339();
    {
        let tx = db.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE transformation_chapters SET status='done', tokens_in=?2, tokens_out=?3, \
             result_content=NULL, completed_at=?4 WHERE id=?1",
            rusqlite::params![tid, tokens_in, tokens_out, now],
        )?;
        // 同步写入结果槽
        tx.execute(
            "UPDATE workflow_result_chapters SET content=?2, updated_at=?3 \
             WHERE workflow_result_id = (SELECT id FROM workflow_results WHERE batch_id = ?4) \
               AND chapter_id = ?1",
            rusqlite::params![tc.chapter_id, content, now, batch_id],
        )?;
        tx.commit()?;
    }
    self.advance_batch(&db, batch_id)
}
```

并更新 `lib.rs` 的 `Notifier` 闭包,把 worker 返回的 `content` 透传到 `on_chapter_done`。

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test -p nsc-core --test queue
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/nsc-core/src/transformer/queue.rs crates/nsc-core/src/transformer/batch_scheduler.rs \
        src-tauri/src/lib.rs crates/nsc-core/tests/queue.rs
git commit -m "feat(scheduler): success writes workflow result slot, not tc.result_content"
```

---

## Task 6: 当前工作流上下文（仅读结果集）

**Files:**
- Modify: `crates/nsc-core/src/transformer/batch_scheduler.rs`（`dispatch` 内的 frontier 计算）
- Test: `crates/nsc-core/tests/scheduler.rs`

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn frontier_only_reads_current_workflow_result_set() {
    let (sched, _db) = setup_scheduler(/* LLM: succeed */);
    let tn_id = /* seed TN */;
    let _other_batch = sched.create_workflow(/* chapter 1 */).unwrap().id;
    wait_until_idle(&sched);
    // 第二个工作流只包含 chapter 2
    let batch_id = sched.create_workflow(/* chapter_ids: [2] */).unwrap().id;
    wait_until_idle(&sched);
    let tc = sched.db.transformation_chapters().list_by_batch(batch_id).unwrap().remove(0);
    assert!(tc.style_ref_chapter_id.is_none(),
        "当前工作流没有前序结果,不应跨工作流引用");
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test -p nsc-core --test scheduler
```

Expected: FAIL（旧 frontier 跨工作流）。

- [ ] **Step 3: 替换 frontier 查询**

```rust
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
         ORDER BY c.idx DESC LIMIT 1"
    )?;
    let mut rows = stmt.query(rusqlite::params![batch_id, chapter_id])?;
    if let Some(row) = rows.next()? { Ok(Some(row.get(0)?)) } else { Ok(None) }
}
```

`create_workflow` 在写入任务时改为调用它（传入新 `batch_id`），不再传 `tn_id`。

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test -p nsc-core --test scheduler
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/nsc-core/src/transformer/batch_scheduler.rs crates/nsc-core/tests/scheduler.rs
git commit -m "feat(scheduler): frontier scoped to current workflow"
```

---

## Task 7: 人工停止 + Stopped 后重试空槽

**Files:**
- Modify: `crates/nsc-core/src/transformer/batch_scheduler.rs`
- Test: `crates/nsc-core/tests/scheduler.rs`

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn stop_marks_pending_skipped_and_finalizes_running_via_callback() {
    let (sched, _db) = setup_scheduler(/* LLM: slow success */);
    let batch_id = sched.create_workflow(/* chapter_ids: [1,2,3] */).unwrap().id;
    sched.stop_workflow(batch_id).unwrap();
    let statuses: Vec<_> = sched.db.transformation_chapters()
        .list_by_batch(batch_id).unwrap().iter().map(|t| t.status).collect();
    // 假设首章在停止瞬间仍 Running,后续章节都 pending → 全部最终为 Done/Skipped
    assert!(statuses.contains(&TransformStatus::Skipped));
    assert_eq!(sched.batch_status(batch_id).unwrap(), BatchStatus::Stopped);
    // 重试空槽:选择所有 skipped 章节
    let skipped_ids: Vec<i64> = sched.db.transformation_chapters().list_by_batch(batch_id)
        .unwrap().iter().filter(|t| t.status == TransformStatus::Skipped)
        .map(|t| t.chapter_id).collect();
    sched.retry_empty_slots(batch_id, &skipped_ids).unwrap();
    wait_until_idle(&sched);
    assert_eq!(sched.batch_status(batch_id).unwrap(), BatchStatus::Stopped);
    for tc in sched.db.transformation_chapters().list_by_batch(batch_id).unwrap() {
        assert_ne!(tc.status, TransformStatus::Skipped, "重试后应变为 Done");
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test -p nsc-core --test scheduler
```

Expected: FAIL（`stop_workflow` 不存在）。

- [ ] **Step 3: 实现 `stop_workflow` + `retry_empty_slots`**

```rust
pub fn stop_workflow(&self, batch_id: i64) -> Result<Batch> {
    let db = Db::open(&self.db_path)?;
    let batch = db.batches().get(batch_id)?
        .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
    if matches!(batch.status, BatchStatus::Stopped) {
        return Ok(batch);  // 幂等
    }
    if !matches!(batch.status, BatchStatus::Running) {
        return Err(Error::Validation(format!("batch {batch_id} 状态 {:?} 不能 stop", batch.status)));
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
    drop(tx);
    let updated = db.batches().get(batch_id)?
        .ok_or_else(|| Error::NotFound("batch 回读失败".into()))?;
    Ok(updated)
}

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
    let now = Utc::now().to_rfc3339();
    let (mut first_tid, first_cid) = {
        let tx = db.conn.unchecked_transaction()?;
        // 验证所选章节都属于该 batch 且结果槽为空、任务为 Failed/Skipped
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
                return Err(Error::Validation(format!("章节 {cid} 不是可重试空槽")));
            }
        }
        tx.execute(
            "UPDATE batches SET status='running', ended_at=NULL WHERE id=?1",
            rusqlite::params![batch_id],
        )?;
        // 取最小 idx 章节作为首派
        let (first_tid, first_cid): (i64, i64) = tx.query_row(
            "SELECT tc.id, tc.chapter_id FROM transformation_chapters tc \
             JOIN chapters c ON c.id = tc.chapter_id \
             WHERE tc.batch_id=?1 AND tc.status='pending' \
             ORDER BY c.idx ASC LIMIT 1",
            rusqlite::params![batch_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        tx.commit()?;
        (first_tid, first_cid)
    };
    let _ = first_cid;
    let tn = db.transformation_novels().get(batch.transformation_novel_id)?
        .ok_or_else(|| Error::NotFound(format!("tn {} 不存在", batch.transformation_novel_id)))?;
    let prompt_id = tc_prompt_id(&db, first_tid)?;
    let model_id = tc_model_id(&db, first_tid)?;
    let prompt = db.prompts().get(prompt_id)?
        .ok_or_else(|| Error::NotFound(format!("prompt {prompt_id} 不存在")))?;
    let model = db.model_configs().get(model_id)?
        .ok_or_else(|| Error::NotFound(format!("model {model_id} 不存在")))?;
    self.dispatch(&db, &tn, &prompt, &model, first_tid, 0, 0, 0)?;
    let updated = db.batches().get(batch_id)?
        .ok_or_else(|| Error::NotFound("batch 回读失败".into()))?;
    Ok(updated)
}

fn tc_prompt_id(db: &Db, tid: i64) -> Result<i64> {
    db.conn.query_row(
        "SELECT prompt_id FROM transformation_chapters WHERE id=?1",
        rusqlite::params![tid], |r| r.get(0),
    ).map_err(Into::into)
}

fn tc_model_id(db: &Db, tid: i64) -> Result<i64> {
    db.conn.query_row(
        "SELECT model_config_id FROM transformation_chapters WHERE id=?1",
        rusqlite::params![tid], |r| r.get(0),
    ).map_err(Into::into)
}
```

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test -p nsc-core --test scheduler
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/nsc-core/src/transformer/batch_scheduler.rs crates/nsc-core/tests/scheduler.rs
git commit -m "feat(scheduler): stop_workflow + retry_empty_slots"
```

---

## Task 8: 启动安全恢复

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/src/commands/recovery.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Test: `crates/nsc-core/tests/startup_recovery.rs`

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn startup_recovery_marks_running_chapters_failed_and_workflow_stopped() {
    let db = Db::open_in_memory().unwrap();
    /* seed TN + batch(running) + tc(running) + tc(pending) */
    nsc_core::startup_recovery::run(&db.conn).unwrap();
    let statuses: Vec<_> = db.transformation_chapters().list_by_batch(batch_id).unwrap()
        .iter().map(|t| t.status).collect();
    assert!(statuses.contains(&TransformStatus::Failed));
    assert!(statuses.contains(&TransformStatus::Skipped));
    assert!(matches!(db.batches().get(batch_id).unwrap().unwrap().status, BatchStatus::Stopped));
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test -p nsc-core --test startup_recovery
```

Expected: FAIL（`startup_recovery` 模块不存在）。

- [ ] **Step 3: 实现 startup_recovery 模块**

在 `crates/nsc-core/src/lib.rs` 注册新模块并写：

```rust
// crates/nsc-core/src/startup_recovery.rs
use crate::db::Db;
use crate::error::Result;

pub fn run(conn: &rusqlite::Connection) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE transformation_chapters SET status='failed', error='进程中断,安全停止',
            completed_at=COALESCE(completed_at, ?1)
         WHERE status='running'",
        rusqlite::params![now],
    )?;
    tx.execute(
        "UPDATE transformation_chapters SET status='skipped', completed_at=COALESCE(completed_at, ?1)
         WHERE status='pending'",
        rusqlite::params![now],
    )?;
    tx.execute(
        "UPDATE batches SET status='stopped', ended_at = COALESCE(ended_at, started_at, created_at)
         WHERE status='running'",
        [],
    )?;
    tx.commit()?;
    Ok(())
}
```

并在 `src-tauri/src/lib.rs::run` 中、`Db::open` 后立即调用 `nsc_core::startup_recovery::run(&db.conn)`。

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test -p nsc-core --test startup_recovery
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/nsc-core/src/startup_recovery.rs crates/nsc-core/src/lib.rs src-tauri/src/lib.rs \
        crates/nsc-core/tests/startup_recovery.rs
git commit -m "feat(core): startup safe-recovery for orphan running workflows"
```

---

## Task 9: IPC 命令重塑（workflow 域）

**Files:**
- Create: `src-tauri/src/commands/workflows.rs`
- Modify: `src-tauri/src/lib.rs`（注册新命令）
- Modify: `src/ipc/commands.ts`
- Modify: `src/ipc/types.ts`
- Test: `src/__tests__/workflows.spec.ts`

- [ ] **Step 1: 写失败测试**

```ts
// src/__tests__/workflows.spec.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import {
  createWorkflow, listWorkflows, getWorkflow, listWorkflowChapters,
  stopWorkflow, retryWorkflowChapters, listTransformationSourceChapters,
  listChapterWorkflowResults,
} from '../ipc/commands';

beforeEach(() => (invoke as any).mockReset());

it('createWorkflow sends create_workflow + snake_case payload', async () => {
  (invoke as any).mockResolvedValueOnce({ id: 1 });
  await createWorkflow({ tn_id: 1, label: 'v1', chapter_ids: [1, 2],
    prompt_id: 1, model_config_id: 1, mode: 'compress',
    ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0 });
  expect(invoke).toHaveBeenCalledWith('create_workflow', {
    payload: { tn_id: 1, label: 'v1', chapter_ids: [1, 2],
      prompt_id: 1, model_config_id: 1, mode: 'compress',
      ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0 },
  });
});

it('stopWorkflow invokes stop_workflow with batchId', async () => {
  (invoke as any).mockResolvedValueOnce({ id: 2 });
  await stopWorkflow(2);
  expect(invoke).toHaveBeenCalledWith('stop_workflow', { batchId: 2 });
});

it('retryWorkflowChapters sends { batchId, chapterIds }', async () => {
  (invoke as any).mockResolvedValueOnce({ id: 3 });
  await retryWorkflowChapters(3, [5, 6]);
  expect(invoke).toHaveBeenCalledWith('retry_workflow_chapters', { batchId: 3, chapterIds: [5, 6] });
});
```

- [ ] **Step 2: 运行测试确认失败**

```bash
pnpm test -- workflows.spec
```

Expected: FAIL（wrapper 缺失）。

- [ ] **Step 3: 实现 IPC 类型 + wrapper**

`src/ipc/types.ts`：

```ts
export type WorkflowStatus = 'running' | 'stopped';

export interface WorkflowSummary {
  id: number;
  tn_id: number;
  label: string | null;
  status: WorkflowStatus;
  created_at: string;
  started_at: string | null;
  ended_at: string | null;
  done_count: number;
  failed_count: number;
  skipped_count: number;
  total_count: number;
}

export interface WorkflowChapterRow {
  tc_id: number;
  chapter_id: number;
  chapter_idx: number;
  chapter_title: string;
  status: 'pending' | 'running' | 'done' | 'failed' | 'skipped';
  error: string | null;
  content_preview: string | null;
  is_empty_slot: boolean;
}

export interface SourceChapterRow {
  chapter_id: number;
  idx: number;
  title: string;
  word_count: number;
  non_empty_result_count: number;
}

export interface ChapterWorkflowResultRow {
  batch_id: number;
  batch_label: string | null;
  batch_status: WorkflowStatus;
  batch_ended_at: string | null;
  content: string | null;
  status: 'pending' | 'running' | 'done' | 'failed' | 'skipped';
}

export interface CreateWorkflowInput {
  tn_id: number;
  label: string | null;
  chapter_ids: number[];
  prompt_id: number;
  model_config_id: number;
  mode: 'compress' | 'style';
  ctx_prev_original: number;
  ctx_prev_transformed: number;
  ctx_next_original: number;
}
```

`src/ipc/commands.ts` 新增：

```ts
export const createWorkflow = (payload: CreateWorkflowInput): Promise<WorkflowSummary> =>
  invoke<WorkflowSummary>('create_workflow', { payload });
export const listWorkflows = (tnId: number): Promise<WorkflowSummary[]> =>
  invoke<WorkflowSummary[]>('list_workflows', { tnId });
export const getWorkflow = (batchId: number): Promise<WorkflowSummary> =>
  invoke<WorkflowSummary>('get_workflow', { batchId });
export const listWorkflowChapters = (batchId: number): Promise<WorkflowChapterRow[]> =>
  invoke<WorkflowChapterRow[]>('list_workflow_chapters', { batchId });
export const stopWorkflow = (batchId: number): Promise<WorkflowSummary> =>
  invoke<WorkflowSummary>('stop_workflow', { batchId });
export const retryWorkflowChapters = (batchId: number, chapterIds: number[]): Promise<WorkflowSummary> =>
  invoke<WorkflowSummary>('retry_workflow_chapters', { batchId, chapterIds });
export const listTransformationSourceChapters = (tnId: number): Promise<SourceChapterRow[]> =>
  invoke<SourceChapterRow[]>('list_transformation_source_chapters', { tnId });
export const listChapterWorkflowResults = (tnId: number, chapterId: number): Promise<ChapterWorkflowResultRow[]> =>
  invoke<ChapterWorkflowResultRow[]>('list_chapter_workflow_results', { tnId, chapterId });
```

并删除旧的 `createBatch / dispatchBatch / resumeBatch / updateBatch / listBatches` 等 wrapper（保留一个临时 `listBatches` shim 用于渐进迁移仅在本任务期间存在，本任务末尾删除）。

`src-tauri/src/commands/workflows.rs`：

```rust
use std::sync::Arc;
use serde::Deserialize;
use tauri::State;
use nsc_core::db::Db;
use nsc_core::error::Error;
use nsc_core::models::{TransformMode, WorkflowCreate};
use nsc_core::transformer::BatchScheduler;

use std::sync::Mutex;

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
}

impl CreateWorkflowPayload {
    fn into_core(self) -> Result<WorkflowCreate, Error> {
        let mode = match self.mode.as_str() {
            "compress" => TransformMode::Compress,
            "style"    => TransformMode::Style,
            other      => return Err(Error::Validation(format!("未知 mode: {other}"))),
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
        })
    }
}

#[tauri::command]
pub async fn create_workflow(
    payload: CreateWorkflowPayload,
    scheduler: State<'_, Arc<BatchScheduler>>,
) -> Result<WorkflowSummary, String> {
    let sched = scheduler.inner().clone();
    let spec = payload.into_core().map_err(|e| e.to_string())?;
    let res = tokio::task::spawn_blocking(move || sched.create_workflow(spec))
        .await.map_err(|e| format!("create_workflow join: {e}"))?
        .map_err(|e| e.to_string())?;
    Ok(workflow_summary(&res))
}

// list_workflows / get_workflow / list_workflow_chapters / stop_workflow /
// retry_workflow_chapters / list_transformation_source_chapters /
// list_chapter_workflow_results 命令按 spec §8 实现;
```

`workflow_summary` 等序列化辅助函数照 `BatchSummary` 风格单独写在本文件。

最后在 `src-tauri/src/lib.rs` 注册新命令并删除旧的 `commands::batches::create_batch / dispatch_batch / resume_batch`。

- [ ] **Step 4: 运行测试确认通过**

```bash
pnpm test -- workflows.spec
cargo test -p nsc-core --test scheduler
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/commands/workflows.rs src-tauri/src/lib.rs \
        src/ipc/commands.ts src/ipc/types.ts \
        src/__tests__/workflows.spec.ts
git commit -m "feat(ipc): workflow domain commands + frontend wrappers"
```

---

## Task 10: 前端 store / 详情页切换到 workflow 数据源

**Files:**
- Modify: `src/views/TransformationNovelDetail.vue`
- Modify: `src/components/CreateBatchDialog.vue`
- Delete: `src/stores/batches.ts`（被 `workflows.ts` 替代）
- Create: `src/stores/workflows.ts`
- Test: `src/__tests__/tnDetailWorkflow.spec.ts`

- [ ] **Step 1: 写失败测试**

```ts
// src/__tests__/tnDetailWorkflow.spec.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { useWorkflowsStore } from '../stores/workflows';

beforeEach(() => (invoke as any).mockReset());

it('createAndRun invokes create_workflow and refreshes byTn', async () => {
  (invoke as any).mockResolvedValueOnce({ id: 9, tn_id: 3, status: 'running', done_count: 0, failed_count: 0, skipped_count: 0, total_count: 2 });
  const store = useWorkflowsStore();
  const w = await store.createAndRun({ tn_id: 3, label: null, chapter_ids: [1, 2], prompt_id: 1, model_config_id: 1, mode: 'compress', ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0 });
  expect(w.id).toBe(9);
  expect(invoke).toHaveBeenCalledWith('create_workflow', expect.any(Object));
  expect(store.byTn.get(3)?.[0].id).toBe(9);
});
```

- [ ] **Step 2: 运行测试确认失败**

```bash
pnpm test -- tnDetailWorkflow.spec
```

Expected: FAIL（`useWorkflowsStore` 不存在）。

- [ ] **Step 3: 实现 store + 详情页改造**

`src/stores/workflows.ts`：

```ts
import { defineStore } from 'pinia';
import { ref } from 'vue';
import {
  createWorkflow, listWorkflows, getWorkflow, stopWorkflow,
  retryWorkflowChapters, listWorkflowChapters, listTransformationSourceChapters,
  listChapterWorkflowResults,
} from '../ipc/commands';
import type {
  CreateWorkflowInput, WorkflowSummary, WorkflowChapterRow, SourceChapterRow,
  ChapterWorkflowResultRow,
} from '../ipc/types';

export const useWorkflowsStore = defineStore('workflows', () => {
  const byTn = ref<Map<number, WorkflowSummary[]>>(new Map());
  const chaptersByBatch = ref<Map<number, WorkflowChapterRow[]>>(new Map());
  const sourcesByTn = ref<Map<number, SourceChapterRow[]>>(new Map());
  const resultsByTnChapter = ref<Map<string, ChapterWorkflowResultRow[]>>(new Map());
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function loadSources(tnId: number) {
    sourcesByTn.value.set(tnId, await listTransformationSourceChapters(tnId));
  }
  async function loadByTn(tnId: number) {
    byTn.value.set(tnId, await listWorkflows(tnId));
  }
  async function loadChapters(batchId: number) {
    chaptersByBatch.value.set(batchId, await listWorkflowChapters(batchId));
  }
  async function loadResultsForChapter(tnId: number, chapterId: number) {
    resultsByTnChapter.value.set(`${tnId}:${chapterId}`,
      await listChapterWorkflowResults(tnId, chapterId));
  }
  async function createAndRun(payload: CreateWorkflowInput): Promise<WorkflowSummary> {
    loading.value = true;
    try {
      const w = await createWorkflow(payload);
      const list = byTn.value.get(w.tn_id) ?? [];
      list.unshift(w);
      byTn.value.set(w.tn_id, list);
      return w;
    } finally { loading.value = false; }
  }
  async function refresh(batchId: number) {
    const w = await getWorkflow(batchId);
    const list = byTn.value.get(w.tn_id);
    if (list) {
      const i = list.findIndex(x => x.id === batchId);
      if (i >= 0) list[i] = w; else list.unshift(w);
    }
  }
  async function stop(batchId: number) {
    const w = await stopWorkflow(batchId);
    await refresh(batchId);
    return w;
  }
  async function retry(batchId: number, chapterIds: number[]) {
    const w = await retryWorkflowChapters(batchId, chapterIds);
    await refresh(batchId);
    return w;
  }
  return {
    byTn, chaptersByBatch, sourcesByTn, resultsByTnChapter,
    loading, error, loadSources, loadByTn, loadChapters, loadResultsForChapter,
    createAndRun, refresh, stop, retry,
  };
});
```

`src/views/TransformationNovelDetail.vue` 改造：

- 章节一览 tab 数据源改为 `store.sourcesByTn`（`SourceChapterRow`），列改为：勾选、序号、标题、字数、已有结果数（=`non_empty_result_count`）。
- 表头动作：全选 / 全不选 / 反选 / “▶ 新建工作流（N 章）”。
- 点击标题打开章节详情，调用 `loadResultsForChapter`。
- 工作流 tab 数据源改为 `store.byTn`，列：label、状态、总章节数、Done / Failed / Skipped 数、创建/结束时间。
- 点击工作流加载 `chaptersByBatch`，Running 时显示“停止工作流”，Stopped 时允许勾选 `Failed/Skipped` 且 `is_empty_slot` 行，“重试所选”按钮调用 `retry`。

`src/components/CreateBatchDialog.vue`：

- 删除失败策略字段。
- 新增 prop `selectedChapterIds: number[]`,展示“已选 N 章”。
- 提交 payload 改为 `CreateWorkflowInput`,移除 `on_failure_policy`。

并删除 `src/stores/batches.ts`。

- [ ] **Step 4: 运行测试确认通过**

```bash
pnpm test -- tnDetailWorkflow
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src/views/TransformationNovelDetail.vue src/components/CreateBatchDialog.vue \
        src/stores/workflows.ts src/stores/batches.ts \
        src/__tests__/tnDetailWorkflow.spec.ts
git commit -m "feat(ui): detail page + dialog switched to workflow data source"
```

---

## Task 11: 集成与手工验证

**Files:** 无新增文件，仅校验。

- [ ] **Step 1: 跑全部自动化测试**

```bash
pnpm test
cargo test -p nsc-core
```

Expected: PASS。

- [ ] **Step 2: 启动 Tauri/Vite 实操**

```bash
pnpm tauri dev
```

按 spec §11.3 走 6 个路径：

1. 同一章节同时加入两个工作流,得到两份独立结果;
2. 一个工作流内失败后继续下一章;
3. 人工停止时当前章完成、后续章 Skipped;
4. Stopped 后重试空槽;
5. 章节详情按工作流展示多份结果;
6. 源数据资产内容始终未改变。

记录失败 case 至 issues。

- [ ] **Step 3: 完成判据自查**

逐条核对 spec §12,在 PR 描述里写明每条判据的证据。

- [ ] **Step 4: 提交任何残留修复并 push**

```bash
git commit --allow-empty -m "chore: workflow results integration verified"
git push
```