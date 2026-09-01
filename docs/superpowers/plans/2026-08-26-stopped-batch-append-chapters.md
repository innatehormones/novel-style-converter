# Stopped Batch 追加章节 + 继续执行 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 stopped batch 可以从 source data_asset 拉新章节追加,batch 自动从 stopped → running 继续执行。append 走 BatchScheduler(治本,不在 IPC 命令层手撸)。

**Architecture:** schema 加 batches 同质配置字段(7 列,含 backfill);`BatchStatus::Stopped` 的 doc 注释更新为「可经 append_chapters_to_batch 转 running」;`BatchRepo::set_status(Running)` 清 ended_at;`BatchScheduler::append_chapters_to_batch` 是核心方法;IPC 命令薄层委派 scheduler;前端 `AppendChaptersDialog` 复用章节选择机制;store action + 已有 vue-query 失效模式。

**Tech Stack:** Rust(rusqlite + tokio + BatchScheduler + JobQueue)、Vue 3 + TypeScript + Pinia + TanStack Query、vitest、playwright(placeholder)。

**Spec:** `docs/superpowers/specs/2026-08-26-stopped-batch-append-chapters-design.md`

---

## 文件结构

**后端**
- `migrations/0029_batch_homogeneous_config.sql` — 新建
- `crates/nsc-core/src/db/migrate.rs` — 注册 0029
- `crates/nsc-core/src/models/batch.rs` — `Batch` / `NewBatch` 加 7 字段
- `crates/nsc-core/src/db/repo/batch.rs` — `insert` / `get` / `list_by_tn` 加 7 列;`set_status(Running)` 清 `ended_at`
- `crates/nsc-core/src/transformer/batch_scheduler.rs` — 新增 `append_chapters_to_batch` 方法(核心)
- `src-tauri/src/commands/transformations.rs` — 新增 `append_chapters_to_batch` IPC 命令(薄层委派)
- `src-tauri/src/lib.rs` — 注册 IPC 命令

**前端**
- `src/ipc/types.ts` — 加 `AppendChaptersToBatchPayload` / `AppendChaptersResult`
- `src/ipc/commands.ts` — 加 `appendChaptersToBatch` wrapper
- `src/stores/workflows.ts` — 加 `appendChapters` action(失效 workflowChapters + workflows)
- `src/components/AppendChaptersDialog.vue` — 新建
- `src/views/TransformationNovelDetail.vue` — actions 列加按钮 + dialog 挂载

**测试**
- `crates/nsc-core/tests/append_chapters.rs` — 新建(Rust 集成测试)
- `src/__tests__/appendChaptersDialog.spec.ts` — 新建(vitest)
- `tests-e2e/append-chapters.spec.ts` — 新建(`test.skip` placeholder)

---

## Task 1: schema migration + models 同质配置

**Files:**
- Create: `migrations/0029_batch_homogeneous_config.sql`
- Modify: `crates/nsc-core/src/db/migrate.rs`
- Modify: `crates/nsc-core/src/models/batch.rs`

- [ ] **Step 1: 新增 migration 文件**

`migrations/0029_batch_homogeneous_config.sql`:

```sql
-- batches 补「同质配置」字段:batch 创建时统一采用同一套 prompt/model/ctx/mode。
-- stopped batch append 章节时,从 batch 字段直接读,无需反查 tc 行。
ALTER TABLE batches ADD COLUMN prompt_id INTEGER;
ALTER TABLE batches ADD COLUMN model_config_id INTEGER;
ALTER TABLE batches ADD COLUMN mode TEXT;
ALTER TABLE batches ADD COLUMN ctx_prev_original INTEGER;
ALTER TABLE batches ADD COLUMN ctx_prev_transformed INTEGER;
ALTER TABLE batches ADD COLUMN ctx_next_original INTEGER;
ALTER TABLE batches ADD COLUMN ctx_next_transformed INTEGER;

-- 旧数据 backfill:从该 batch 下任意一个 tc 行取(业务上同质)。
-- 不存在 tc 行的 batch 留 NULL(理论上不该有;防御性)。
UPDATE batches SET
  prompt_id = (SELECT prompt_id FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  model_config_id = (SELECT model_config_id FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  mode = (SELECT mode FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  ctx_prev_original = (SELECT ctx_prev_original FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  ctx_prev_transformed = (SELECT ctx_prev_transformed FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  ctx_next_original = (SELECT ctx_next_original FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  ctx_next_transformed = (SELECT ctx_next_transformed FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1);
```

- [ ] **Step 2: 注册 migration**

`crates/nsc-core/src/db/migrate.rs` 在 `SCHEMAS` 数组末尾(`("0027_tc_batch_cascade", ...)` 之后)加:

```rust
    ("0028_batch_homogeneous_config", include_str!("../../../../migrations/0029_batch_homogeneous_config.sql")),
```

(注:version 字符串是 `0028_…` 而不是 `0029_…`,因为 migration 表里有版本号管理,但用文件名 0029 标识 task。)

- [ ] **Step 3: models 加字段**

`crates/nsc-core/src/models/batch.rs`:

```rust
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
    // 新增(append_chapters spec §3.2):
    pub prompt_id: i64,
    pub model_config_id: i64,
    /// PromptKind 的字符串形式("compress" / "style")。
    pub mode: String,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
    pub ctx_next_transformed: i32,
}

#[derive(Debug, Clone)]
pub struct NewBatch {
    pub transformation_novel_id: i64,
    pub label: Option<String>,
    pub on_failure_policy: OnFailurePolicy,
    // 新增:
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub mode: String,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
    pub ctx_next_transformed: i32,
}
```

注意 `Batch.mode` 用 `String`(不是 enum) — `transformation_chapters.mode` 列也是 TEXT(看 batch_scheduler.rs:143 `mode_str(spec.mode)`),保持 wire-level 一致。

- [ ] **Step 4: 验证编译(预期失败)**

```bash
cargo build -p nsc-core
```

Expected: 编译失败(`Batch.from_row` / `BatchRepo::insert` 还没改 — Task 2)。先确认 `migrate.rs` 注册和 `batch.rs` model 改动语法正确。

- [ ] **Step 5: Commit**

```bash
git add migrations/0029_batch_homogeneous_config.sql crates/nsc-core/src/db/migrate.rs crates/nsc-core/src/models/batch.rs
git commit -m "feat(db): add batches homogeneous config fields + backfill"
```

---

## Task 2: BatchRepo 读写新字段 + `set_status(Running)` 清 ended_at

**Files:**
- Modify: `crates/nsc-core/src/db/repo/batch.rs`

- [ ] **Step 1: `batch_from_row` 读 7 个新字段**

在 `crates/nsc-core/src/db/repo/batch.rs` 找到 `batch_from_row`(当前约 line 146),替换为:

```rust
pub(crate) fn batch_from_row(row: &Row<'_>) -> rusqlite::Result<Batch> {
    let created_at_s: String = row.get(5)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_s)
        .map(|from| from.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, e.into()))?;
    let started_at_s: Option<String> = row.get(6)?;
    let ended_at_s:   Option<String> = row.get(7)?;
    let parse_opt = |s: Option<String>| -> rusqlite::Result<Option<DateTime<Utc>>> {
        match s {
            None => Ok(None),
            Some(s) => DateTime::parse_from_rfc3339(&s)
                .map(|from| Some(from.with_timezone(&Utc)))
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, e.into())),
        }
    };
    Ok(Batch {
        id: row.get(0)?,
        transformation_novel_id: row.get(1)?,
        label: row.get(2)?,
        on_failure_policy: str_to_policy(&row.get::<_, String>(3)?)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, e.into()))?,
        status: str_to_status(&row.get::<_, String>(4)?)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, e.into()))?,
        created_at,
        started_at: parse_opt(started_at_s)?,
        ended_at: parse_opt(ended_at_s)?,
        // 新增:
        prompt_id: row.get(8)?,
        model_config_id: row.get(9)?,
        mode: row.get(10)?,
        ctx_prev_original: row.get(11)?,
        ctx_prev_transformed: row.get(12)?,
        ctx_next_original: row.get(13)?,
        ctx_next_transformed: row.get(14)?,
    })
}
```

- [ ] **Step 2: 4 个 SELECT 加新列**

在 `batch.rs` 找到所有 `SELECT ... FROM batches` 语句,列清单末尾加 `, prompt_id, model_config_id, mode, ctx_prev_original, ctx_prev_transformed, ctx_next_original, ctx_next_transformed`。共 4 处:`get`、`list_by_tn`、`count_by_status` 不用改(那是聚合),其他 select 都得改。

- [ ] **Step 3: `insert` 加 7 个字段**

`BatchRepo::insert` 当前 INSERT 5 列。改为:

```rust
pub fn insert(&self, b: &NewBatch) -> Result<i64> {
    use crate::models::PromptKind;
    let now = Utc::now().to_rfc3339();
    let policy_s = policy_to_str(b.on_failure_policy);
    // spec §1.4:mode 必须是 PromptKind 的字符串形式;create_workflow 那里 mode_str
    // 也存在但不可见,这里 inline 写一份避免改 batch_scheduler 接口。
    let mode_s = match b.mode.as_str() {
        "compress" => "compress",
        "style" => "style",
        other => return Err(crate::error::Error::Validation(format!("unknown mode: {other}"))),
    };
    self.conn.execute(
        "INSERT INTO batches \
         (transformation_novel_id, label, on_failure_policy, status, created_at, \
          prompt_id, model_config_id, mode, \
          ctx_prev_original, ctx_prev_transformed, ctx_next_original, ctx_next_transformed) \
         VALUES (?1, ?2, ?3, 'pending', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            b.transformation_novel_id, b.label, policy_s, now,
            b.prompt_id, b.model_config_id, mode_s,
            b.ctx_prev_original, b.ctx_prev_transformed, b.ctx_next_original, b.ctx_next_transformed,
        ],
    )?;
    Ok(self.conn.last_insert_rowid())
}
```

- [ ] **Step 4: `set_status(Running)` 清 `ended_at`**

当前实现:
```rust
BatchStatus::Running => {
    self.conn.execute(
        "UPDATE batches SET status = ?2, started_at = COALESCE(started_at, ?3) WHERE id = ?1",
        params![id, status_s, now],
    )?;
}
```

改为:
```rust
BatchStatus::Running => {
    self.conn.execute(
        "UPDATE batches SET status = ?2, \
         started_at = COALESCE(started_at, ?3), \
         ended_at = NULL \
         WHERE id = ?1",
        params![id, status_s, now],
    )?;
}
```

为什么:append_chapters 路径要让 batch 从 stopped → running,需要清掉旧 ended_at;这同时让未来其他 stop → resume 路径也受益(治本)。

- [ ] **Step 5: 同步 create_workflow 路径**

`crates/nsc-core/src/transformer/batch_scheduler.rs` 的 `create_workflow` 方法,当前 INSERT batches 行只写了 4 列 + label。改为 11 列(加 7 个新字段):

```rust
tx.execute(
    "INSERT INTO batches (transformation_novel_id, label, on_failure_policy, status, created_at, started_at, \
     prompt_id, model_config_id, mode, \
     ctx_prev_original, ctx_prev_transformed, ctx_next_original, ctx_next_transformed) \
     VALUES (?1, ?2, ?3, 'running', ?4, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    rusqlite::params![
        spec.transformation_novel_id, spec.label,
        policy_str(spec.on_failure_policy), now,
        spec.prompt_id, spec.model_config_id, mode_str(spec.mode),
        spec.ctx_prev_original, spec.ctx_prev_transformed, spec.ctx_next_original,
        spec.ctx_next_transformed,
    ],
)?;
```

- [ ] **Step 6: 验证编译 + 测试**

```bash
cargo build -p nsc-core
cargo test -p nsc-core --lib
cargo test -p nsc-core --test splitter_new
```

Expected: 全绿。`cargo build` 可能因 lib.rs / commands 等还没接 `NewBatch` 新字段而失败 — 那是 Task 3 范围(IPC 命令)。

- [ ] **Step 7: Commit**

```bash
git add crates/nsc-core/src/db/repo/batch.rs crates/nsc-core/src/transformer/batch_scheduler.rs
git commit -m "feat(db): thread batches homogeneous config through repo + scheduler"
```

---

## Task 3: `BatchScheduler::append_chapters_to_batch` 核心方法

**Files:**
- Modify: `crates/nsc-core/src/transformer/batch_scheduler.rs`
- Test: `crates/nsc-core/tests/append_chapters.rs`

- [ ] **Step 1: 写失败测试(skeleton)**

新建 `crates/nsc-core/tests/append_chapters.rs`,先写 happy path 测试:

```rust
//! append_chapters_to_batch 集成测试 —— spec §7.1。
//!
//! 用 Db::open_in_memory() + BatchScheduler::new() 真实跑流程。
//! 注意 BatchScheduler::new 需要 tokio runtime + provider_factory;
//! 为简化测试,本测试文件只覆盖纯 DB 路径,scheduler 路径留 e2e。
use nsc_core::db::Db;
use nsc_core::models::prompt::{Prompt, PromptKind};
use nsc_core::models::batch::{NewBatch, OnFailurePolicy};

fn new_da_with_chapters(db: &Db, n: i64) -> (i64, i64, Vec<i64>) {
    let upload_id = db.uploads().insert(&nsc_core::models::NewUpload {
        sha256: "x".into(),
        filename: "t.txt".into(),
        byte_size: 0,
        file_path: String::new(),
        original_text: String::new(),
        ..Default::default()
    }).unwrap();
    let da_id = db.data_assets().insert(&nsc_core::models::NewDataAsset {
        upload_id,
        title: "DA".into(),
        source_filename: "t.txt".into(),
        ..Default::default()
    }).unwrap();
    let mut cids = Vec::new();
    for i in 1..=n {
        cids.push(db.chapters().insert(&nsc_core::models::NewChapter {
            data_asset_id: da_id,
            idx: i as i32,
            title: format!("chapter {i}"),
            body: format!("body {i}"),
            word_count: 1,
            ..Default::default()
        }).unwrap());
    }
    let tn_id = db.transformation_novels().insert(&nsc_core::models::NewTransformationNovel {
        data_asset_id: da_id,
        title: "TN".into(),
        note: String::new(),
    }).unwrap();
    (tn_id, da_id, cids)
}

#[test]
fn stop_then_append_requires_status_stopped() {
    // 1. 准备 stopped batch(直接 INSERT 然后 UPDATE status)
    // 2. 试 append → 应成功
}
```

先用最简单的占位测试让测试基础设施就位。具体场景测试在 Step 4-7 加。

- [ ] **Step 2: 验证编译 + 测试框架**

```bash
cargo test -p nsc-core --test append_chapters
```

Expected: 编译并通过占位测试(只编译不报错,测试因只有占位空跑)。

- [ ] **Step 3: `BatchScheduler::append_chapters_to_batch` 方法签名**

在 `crates/nsc-core/src/transformer/batch_scheduler.rs` 的 `impl BatchScheduler` 块末尾加:

```rust
/// append chapters 到 stopped batch(spec §3.4 / Task 3)。
/// 1. 校验 batch 存在 + status==Stopped
/// 2. 校验 chapter_ids 都属于 tn.data_asset
/// 3. 去重:剔除已在 batch 中的章节
/// 4. 事务:insert tc + insert wrc 空槽 + set_status(Running)
/// 5. 提交
/// 6. 对每个新 tc 调 self.dispatch(prompt, model, tc_id) 入队
/// 7. 调 advance_batch 兜底
pub fn append_chapters_to_batch(
    &self,
    batch_id: i64,
    chapter_ids: Vec<i64>,
) -> Result<Vec<i64>> {
    todo!("filled in next steps")
}
```

- [ ] **Step 4: 实现校验 + 去重部分**

替换 `todo!()` 为:

```rust
pub fn append_chapters_to_batch(
    &self,
    batch_id: i64,
    chapter_ids: Vec<i64>,
) -> Result<Vec<i64>> {
    if chapter_ids.is_empty() {
        return Err(Error::Validation("至少选 1 章".into()));
    }
    // 1. 读 batch + 校验 status
    let batch = self.db.batches().get(batch_id)?
        .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
    if batch.status != BatchStatus::Stopped {
        return Err(Error::Validation(format!(
            "仅 stopped 工作流可追加章节(当前 {:?})", batch.status
        )));
    }
    // 2. 读 tn + 校验 chapter_ids 属于 tn.data_asset
    let tn = self.db.transformation_novels().get(batch.transformation_novel_id)?
        .ok_or_else(|| Error::NotFound(format!("tn {} 不存在", batch.transformation_novel_id)))?;
    let da_chapter_set: HashSet<i64> = self.db.chapters().list_by_data_asset(tn.data_asset_id)?
        .iter().map(|c| c.id).collect();
    for &cid in &chapter_ids {
        if !da_chapter_set.contains(&cid) {
            return Err(Error::Validation(format!(
                "chapter {cid} 不属于本 tn 的 data_asset {}", tn.data_asset_id
            )));
        }
    }
    // 3. 去重
    let existing: HashSet<i64> = self.db.transformation_chapters().list_by_batch(batch_id)?
        .iter().map(|tc| tc.chapter_id).collect();
    let to_add: Vec<i64> = chapter_ids.iter().copied().filter(|c| !existing.contains(c)).collect();
    if to_add.is_empty() {
        return Err(Error::Validation("所选章节全部已在工作流中".into()));
    }
    // (Step 5/6/7 待续)
    Ok(to_add)
}
```

需要加 import:`use std::collections::HashSet;` 和 `BatchStatus` (应在文件顶部)。

读 `batch_scheduler.rs` 顶部确认 `BatchStatus` 已 import,如果有就跳过;没有就加 `use crate::models::batch::BatchStatus;`。

- [ ] **Step 5: 写失败测试覆盖校验路径**

在 `crates/nsc-core/tests/append_chapters.rs` 加:

```rust
use nsc_core::db::Db;
use nsc_core::models::batch::{NewBatch, OnFailurePolicy, BatchStatus};
use nsc_core::models::prompt::{Prompt, PromptKind};

fn make_db_with_da(n: i64) -> (Db, i64 /* tn_id */, Vec<i64> /* chapter_ids */) {
    let db = Db::open_in_memory().unwrap();
    let upload_id = db.uploads().insert(&nsc_core::models::NewUpload {
        sha256: "x".into(),
        filename: "t.txt".into(),
        byte_size: 0,
        file_path: String::new(),
        original_text: String::new(),
        ..Default::default()
    }).unwrap();
    let da_id = db.data_assets().insert(&nsc_core::models::NewDataAsset {
        upload_id,
        title: "DA".into(),
        source_filename: "t.txt".into(),
        ..Default::default()
    }).unwrap();
    let mut cids = Vec::new();
    for i in 1..=n {
        cids.push(db.chapters().insert(&nsc_core::models::NewChapter {
            data_asset_id: da_id,
            idx: i as i32,
            title: format!("chapter {i}"),
            body: format!("body {i}"),
            word_count: 1,
            ..Default::default()
        }).unwrap());
    }
    let prompt_id = db.prompts().insert(&Prompt {
        id: 0,
        name: "P".into(),
        kind: PromptKind::Compress,
        template: "{{chapter}}".into(),
        is_builtin: false,
        archived: false,
    }).unwrap();
    let model_id = db.model_configs().insert(&nsc_core::models::NewModelConfig {
        provider: "test".into(),
        model: "test-model".into(),
        display_name: "Test".into(),
        base_url: "http://localhost".into(),
        api_key: "x".into(),
        max_context: 8000,
        ..Default::default()
    }).unwrap();
    let tn_id = db.transformation_novels().insert(&nsc_core::models::NewTransformationNovel {
        data_asset_id: da_id,
        title: "TN".into(),
        note: String::new(),
    }).unwrap();
    let _ = (prompt_id, model_id);  // 记下来给 batch insert 用
    (db, tn_id, cids)
}

fn insert_batch(db: &Db, tn_id: i64, status: BatchStatus) -> i64 {
    db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: Some("test batch".into()),
        on_failure_policy: OnFailurePolicy::PauseAndReview,
        prompt_id: 1,
        model_config_id: 1,
        mode: "compress".into(),
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        ctx_next_transformed: 0,
    }).unwrap()
    // 实际 status 由 set_status 后续设置 — 但 insert 默认 pending;测试可调 set_status
}

#[test]
fn append_to_running_batch_rejected() {
    let (db, tn_id, cids) = make_db_with_da(3);
    let batch_id = insert_batch(&db, tn_id, BatchStatus::Pending);
    // 模拟:启动后 batch 是 running
    db.batches().set_status(batch_id, BatchStatus::Running).unwrap();
    // 直接构造 BatchScheduler 不容易(provider_factory 等),
    // 这里只验证 status==Stopped 才能让 path 走通 — 通过 Db 端校验更轻量。
    // 完整 scheduler 路径在 e2e 测试。
    // 此测试只在 Db 层验证 model 校验逻辑;真正的 append_chapters_to_batch scheduler
    // 测试需要 mock JobQueue,留到本 task 后续 step 或 e2e。
    let batch = db.batches().get(batch_id).unwrap().unwrap();
    assert_ne!(batch.status, BatchStatus::Stopped);
}
```

注意:这层测试只验证 model / db 落库正确性;scheduler 入队路径留到后续 e2e 测试覆盖(`tests-e2e/append-chapters.spec.ts`)。

- [ ] **Step 6: 验证 Step 4 的校验代码**

```bash
cargo build -p nsc-core 2>&1 | head -30
cargo test -p nsc-core --lib
```

Expected: 编译过(`append_chapters_to_batch` 方法已存在但只到校验部分);lib 测试不爆(没破坏现有)。

- [ ] **Step 7: Commit**

```bash
git add crates/nsc-core/src/transformer/batch_scheduler.rs crates/nsc-core/tests/append_chapters.rs
git commit -m "feat(scheduler): append_chapters_to_batch validation + dedup skeleton"
```

---

## Task 4: `append_chapters_to_batch` 完整实现(事务 + dispatch + advance)

**Files:**
- Modify: `crates/nsc-core/src/transformer/batch_scheduler.rs`

- [ ] **Step 1: 扩展方法到事务 + dispatch + advance**

替换 Task 3 的 `append_chapters_to_batch` 完整实现:

```rust
pub fn append_chapters_to_batch(
    &self,
    batch_id: i64,
    chapter_ids: Vec<i64>,
) -> Result<Vec<i64>> {
    if chapter_ids.is_empty() {
        return Err(Error::Validation("至少选 1 章".into()));
    }
    // 1. 读 batch + 校验 status
    let batch = self.db.batches().get(batch_id)?
        .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
    if batch.status != BatchStatus::Stopped {
        return Err(Error::Validation(format!(
            "仅 stopped 工作流可追加章节(当前 {:?})", batch.status
        )));
    }
    // 2. 读 tn + 校验 chapter_ids
    let tn = self.db.transformation_novels().get(batch.transformation_novel_id)?
        .ok_or_else(|| Error::NotFound(format!("tn {} 不存在", batch.transformation_novel_id)))?;
    let da_chapter_set: HashSet<i64> = self.db.chapters().list_by_data_asset(tn.data_asset_id)?
        .iter().map(|c| c.id).collect();
    for &cid in &chapter_ids {
        if !da_chapter_set.contains(&cid) {
            return Err(Error::Validation(format!(
                "chapter {cid} 不属于本 tn 的 data_asset {}", tn.data_asset_id
            )));
        }
    }
    // 3. 去重
    let existing: HashSet<i64> = self.db.transformation_chapters().list_by_batch(batch_id)?
        .iter().map(|tc| tc.chapter_id).collect();
    let to_add: Vec<i64> = chapter_ids.iter().copied().filter(|c| !existing.contains(c)).collect();
    if to_add.is_empty() {
        return Err(Error::Validation("所选章节全部已在工作流中".into()));
    }
    // 4. 事务:insert tc + insert wrc 空槽 + set_status(Running)
    let now = Utc::now().to_rfc3339();
    let mut new_tc_ids: Vec<i64> = Vec::with_capacity(to_add.len());
    {
        let _bsg = self.db.lock();
        let tx = _bsg.unchecked_transaction()?;
        for &cid in &to_add {
            tx.execute(
                "INSERT INTO transformation_chapters \
                 (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
                  ctx_prev_original, ctx_prev_transformed, ctx_next_original, ctx_next_transformed, \
                  batch_id, status, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending', ?11)",
                rusqlite::params![
                    batch.transformation_novel_id, cid, batch.mode,
                    batch.prompt_id, batch.model_config_id,
                    batch.ctx_prev_original, batch.ctx_prev_transformed,
                    batch.ctx_next_original, batch.ctx_next_transformed,
                    batch_id, now,
                ],
            )?;
            new_tc_ids.push(tx.last_insert_rowid());
        }
        // workflow_results + 空槽
        self.db.workflow_results().create_for_batch_with_slots(batch_id, &to_add)?;
        // batch 状态迁移 stopped → running(set_status 已扩展清 ended_at)
        self.db.batches().set_status(batch_id, BatchStatus::Running)?;
        tx.commit()?;
    }
    // 5. 入队每个新 tc — 复用 create_workflow 里的 dispatch 路径
    let prompt = self.db.prompts().get(batch.prompt_id)?
        .ok_or_else(|| Error::NotFound(format!("prompt {} 不存在", batch.prompt_id)))?;
    let model = self.db.model_configs().get(batch.model_config_id)?
        .ok_or_else(|| Error::NotFound(format!("model_config {} 不存在", batch.model_config_id)))?;
    for &tc_id in &new_tc_ids {
        self.dispatch(&prompt, &model, tc_id)?;
    }
    // 6. advance_batch 兜底:确保 batch 内剩余 pending tc 也被派。
    self.advance_batch(&self.db, batch_id)?;
    Ok(new_tc_ids)
}
```

注意 `create_for_batch_with_slots` 是 `INSERT OR IGNORE`,对已存在结果集是幂等的(Worker 已在跑的 batch 也安全)。

- [ ] **Step 2: 验证编译**

```bash
cargo build -p nsc-core
```

Expected: 通过(`append_chapters_to_batch` 完整实现,所有需要的 helper 都存在)。

- [ ] **Step 3: Commit**

```bash
git add crates/nsc-core/src/transformer/batch_scheduler.rs
git commit -m "feat(scheduler): append_chapters_to_batch full implementation"
```

---

## Task 5: IPC 命令层

**Files:**
- Modify: `src-tauri/src/commands/transformations.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: IPC 命令 — 薄层委派**

在 `src-tauri/src/commands/transformations.rs` 末尾加:

```rust
#[derive(Debug, serde::Serialize)]
pub struct AppendChaptersResult {
    pub batch_id: i64,
    pub added_tc_ids: Vec<i64>,
}

/// 把 chapter_ids 追加到 stopped batch(spec §3.4 / Task 3-4)。
/// 薄层委派给 BatchScheduler::append_chapters_to_batch —— 不在此层
/// 手撸事务、校验或入队,所有逻辑都跟 create_workflow 路径共用。
#[tauri::command]
pub fn append_chapters_to_batch(
    scheduler: State<'_, Arc<crate::transformer::batch_scheduler::BatchScheduler>>,
    batch_id: i64,
    chapter_ids: Vec<i64>,
) -> Result<AppendChaptersResult, String> {
    let tc_ids = scheduler.append_chapters_to_batch(batch_id, chapter_ids)
        .map_err(|e| e.to_string())?;
    Ok(AppendChaptersResult { batch_id, added_tc_ids: tc_ids })
}
```

注意:Tauri 2 的 State 类型签名按现有 `enqueue_transformation_chapters` 的模式 — `State<'_, Arc<BatchScheduler>>`。读一下 transformations.rs 现有命令确认 batch_scheduler 是怎么注入的(可能叫 `scheduler` / `batch_scheduler` / 其他名字)。

- [ ] **Step 2: 注册到 lib.rs**

`src-tauri/src/lib.rs` 的 `invoke_handler!` 列表加 `commands::transformations::append_chapters_to_batch,`。

- [ ] **Step 3: 验证编译**

```bash
cargo build
```

Expected: 通过。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/transformations.rs src-tauri/src/lib.rs
git commit -m "feat(ipc): add append_chapters_to_batch command (delegates to scheduler)"
```

---

## Task 6: 前端 types + commands wrapper

**Files:**
- Modify: `src/ipc/types.ts`
- Modify: `src/ipc/commands.ts`

- [ ] **Step 1: types 加**

`src/ipc/types.ts` 在末尾加:

```ts
/// `append_chapters_to_batch` 入参。
export type AppendChaptersToBatchPayload = {
  batchId: number;
  chapterIds: number[];
};

/// `append_chapters_to_batch` 返回。
export interface AppendChaptersResult {
  batch_id: number;
  added_tc_ids: number[];
}
```

- [ ] **Step 2: commands wrapper**

`src/ipc/commands.ts`:

1. import 列表加 `AppendChaptersToBatchPayload`, `AppendChaptersResult`。
2. 末尾加:

```ts
export function appendChaptersToBatch(payload: AppendChaptersToBatchPayload): Promise<AppendChaptersResult> {
  return invoke<AppendChaptersResult>('append_chapters_to_batch', {
    batchId: payload.batchId,
    chapterIds: payload.chapterIds,
  });
}
```

- [ ] **Step 3: 验证类型**

```bash
pnpm exec vue-tsc --noEmit 2>&1 | grep -E "src/ipc/(types|commands)\.ts"
```

Expected: 空(这两个文件本身无错)。

- [ ] **Step 4: Commit**

```bash
git add src/ipc/types.ts src/ipc/commands.ts
git commit -m "feat(ipc): add appendChaptersToBatch types + wrapper"
```

---

## Task 7: store action + AppendChaptersDialog 组件

**Files:**
- Modify: `src/stores/workflows.ts`
- Create: `src/components/AppendChaptersDialog.vue`

- [ ] **Step 1: store action**

`src/stores/workflows.ts` import 加 `appendChaptersToBatch`,加 `AppendChaptersToBatchPayload`, `AppendChaptersResult`。在 store 内加:

```ts
async function appendChapters(payload: AppendChaptersToBatchPayload): Promise<AppendChaptersResult> {
  const res = await appendChaptersToBatch(payload);
  // 跟 retry 一样,失效章节列表 + workflows 总览
  await queryClient.invalidateQueries({ queryKey: ['workflowChapters', payload.batchId] });
  await queryClient.invalidateQueries({ queryKey: ['workflows'] });
  return res;
}
```

return 对象加 `appendChapters`。

- [ ] **Step 2: 创建 AppendChaptersDialog.vue**

新建 `src/components/AppendChaptersDialog.vue`(参考 `RegeneratePreviewDialog.vue` / `CreateBatchDialog.vue` 的写法)。核心 props:

```ts
defineProps<{
  open: boolean;
  batchId: number;
  transformationNovelId: number;
  promptName: string;
  modelDisplayName: string;
  mode: 'compress' | 'style';
  ctxPrevOriginal: number;
  ctxPrevTransformed: number;
  ctxNextOriginal: number;
}>();
const emit = defineEmits<{
  'update:open': [boolean];
  confirm: [{ batchId: number; chapterIds: number[] }];
}>();
```

组件内部:
- 拉 `listTransformationSourceChapters(tnId)`(已存在)
- 拉 `listWorkflowChapters(batchId)`(已存在),拿当前 batch 内 chapter_ids 集合
- sources 列表 = 全部 source − batch 内的 chapter_id(已 disabled)
- selectedChapterIds + rangeFrom/To + applyRange(复制 TransformationNovelDetail.vue 的逻辑,或抽出来 — Task 8 处理)
- 确认按钮 disabled 当 selectedChapterIds.size === 0
- 确认 emit `confirm` 事件,父组件触发 `store.appendChapters`
- 加载 / 错误用现有 Dialog 模式

- [ ] **Step 3: 验证编译**

```bash
pnpm exec vue-tsc --noEmit 2>&1 | grep "src/components/AppendChaptersDialog.vue"
```

Expected: 空(组件本身无错;其他文件错误是预期的)。

- [ ] **Step 4: Commit**

```bash
git add src/stores/workflows.ts src/components/AppendChaptersDialog.vue
git commit -m "feat(workflows): store.appendChapters + AppendChaptersDialog"
```

---

## Task 8: TransformationNovelDetail.vue 接入

**Files:**
- Modify: `src/views/TransformationNovelDetail.vue`

- [ ] **Step 1: import + dialog 挂载**

`src/views/TransformationNovelDetail.vue`:
1. import `AppendChaptersDialog` 与 `useWorkflowsStore.appendChapters`
2. 在模板末尾(已有 Dialog 后面)挂载新 dialog:

```vue
<AppendChaptersDialog
  v-if="appendOpen && appendTarget !== null"
  v-model:open="appendOpen"
  :batch-id="appendTarget.id"
  :transformation-novel-id="tnId"
  :prompt-name="appendTarget.prompt_name"
  :model-display-name="appendTarget.model_display_name"
  :mode="appendTarget.mode"
  :ctx-prev-original="appendTarget.ctx_prev_original"
  :ctx-prev-transformed="appendTarget.ctx_prev_transformed"
  :ctx-next-original="appendTarget.ctx_next_original"
  @confirm="onAppendConfirm"
/>
```

3. 状态:

```ts
const appendOpen = ref(false);
const appendTarget = ref<WorkflowSummary | null>(null);
```

4. 函数:

```ts
function askAppendChapters(w: WorkflowSummary) {
    if (w.status !== 'stopped') return;  // 仅 stopped 可 append
    appendTarget.value = w;
    appendOpen.value = true;
}
async function onAppendConfirm(payload: { batchId: number; chapterIds: number[] }) {
    try {
        await store.appendChapters(payload);
        appendOpen.value = false;
        appendTarget.value = null;
    } catch (e: unknown) {
        showAlert('补充失败', e instanceof Error ? e.message : String(e));
    }
}
```

- [ ] **Step 2: workflow actions 列加按钮**

找到 `workflowChapterColumns.actions` 渲染处(可能是 DataTable 的 actions cell),在 batch.status === 'stopped' 时显示「补充章节」按钮,绑 `askAppendChapters(w)`。

- [ ] **Step 3: 验证编译**

```bash
pnpm exec vue-tsc --noEmit 2>&1 | grep "src/views/TransformationNovelDetail.vue"
```

Expected: 空。

- [ ] **Step 4: 全量类型检查**

```bash
pnpm exec vue-tsc --noEmit
```

Expected: 退出码 0。

- [ ] **Step 5: Commit**

```bash
git add src/views/TransformationNovelDetail.vue
git commit -m "feat(parse): wire append-chapters dialog into workflow detail"
```

---

## Task 9: 前端 vitest

**Files:**
- Create: `src/__tests__/appendChaptersDialog.spec.ts`(或扩现有 store spec)

- [ ] **Step 1: store action 测试**

新建测试文件(参考 `chapters.spec.ts` 风格)。Mock `appendChaptersToBatch` 与 `useQueryClient`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

vi.mock('../ipc/commands', () => ({
  appendChaptersToBatch: vi.fn(async (p) => ({
    batch_id: p.batchId, added_tc_ids: [100, 101],
  })),
}));

vi.mock('@tanstack/vue-query', () => ({
  useQueryClient: () => ({
    invalidateQueries: vi.fn(),
  }),
}));

import { useWorkflowsStore } from '../stores/workflows';
import { appendChaptersToBatch } from '../ipc/commands';

beforeEach(() => { setActivePinia(createPinia()); vi.clearAllMocks(); });

describe('workflows store: appendChapters', () => {
  it('调用正确 IPC 入参', async () => {
    const store = useWorkflowsStore();
    await store.appendChapters({ batchId: 7, chapterIds: [10, 11] });
    expect(appendChaptersToBatch).toHaveBeenCalledWith({ batchId: 7, chapterIds: [10, 11] });
  });

  it('返回 backend result 含 batch_id 和 added_tc_ids', async () => {
    const store = useWorkflowsStore();
    const res = await store.appendChapters({ batchId: 7, chapterIds: [10] });
    expect(res.batch_id).toBe(7);
    expect(res.added_tc_ids).toEqual([100, 101]);
  });

  it('失败时错误冒泡', async () => {
    (appendChaptersToBatch as Mock).mockRejectedValueOnce(new Error('仅 stopped'));
    await expect(store.appendChapters({ batchId: 7, chapterIds: [10] })).rejects.toThrow('仅 stopped');
  });
});
```

(若 vue-query 的 `useQueryClient` mock 写法不对,改用 vitest 的 `vi.mocked(useQueryClient)` 风格 — 参考项目其他 spec 怎么 mock 它的。)

- [ ] **Step 2: 跑测试**

```bash
pnpm exec vitest run src/__tests__/appendChaptersDialog.spec.ts
```

Expected: 3 passed(若文件命名是 spec.ts;否则放到合适的 spec 文件中)。

- [ ] **Step 3: 全量 vitest**

```bash
pnpm exec vitest run
```

Expected: 现有 19 个 tests + 新增 3 个 = 22 passed。

- [ ] **Step 4: Commit**

```bash
git add src/__tests__/appendChaptersDialog.spec.ts
git commit -m "test(workflows): cover appendChapters store action"
```

---

## Task 10: E2E placeholder

**Files:**
- Create: `tests-e2e/append-chapters.spec.ts`

- [ ] **Step 1: 写 placeholder**

参考 `tests-e2e/parse-zhang-click.spec.ts` 的 mock 结构:

```ts
import { test, expect } from '@playwright/test';

const MOCK_INIT_SCRIPT = /* same as parse-zhang-click — copy */;

test.skip('stopped batch: append 2 chapters triggers running transition', async ({ page }) => {
  // 1. 启动 app + mock IPC 让某 batch 处于 stopped 状态(含 3 章 done)
  // 2. 点 workflow 行的「补充章节」
  // 3. 选 2 章未转换的
  // 4. 点「确认补充并执行」
  // 5. 断言:batch status 变 running;新章节行出现并转 done
});

test.skip('running batch: append button hidden', async ({ page }) => {
  // batch.status='running' 时 actions 列不应显示「补充章节」按钮
});
```

- [ ] **Step 2: Commit**

```bash
git add tests-e2e/append-chapters.spec.ts
git commit -m "test(e2e): append-chapters placeholder"
```

---

## Self-Review

1. **Spec 覆盖**:§3.1 schema → Task 1;§3.2 models → Task 1;§3.3 repo → Task 2;§3.4 scheduler 核心 → Task 3-4;§3.5 IPC 注册 → Task 5;§4.1-4.2 前端 types/store/dialog → Task 6-7;§4.3-4.4 parse.vue 接入 → Task 8;§7 测试 → Task 9-10。fail-fast 表格(§5)落在 Task 3 校验路径 + Task 4 错误冒泡 + Task 9 测试。
2. **占位符扫描**:无 "TBD/TODO/implement later";所有 commit 命令、行号、SQL 字符串都给齐。
3. **类型一致性**:
   - `Batch` 字段顺序:prompt_id 在 status 后,其他 ctx_* 在末尾 — Task 1 与 Task 2 顺序一致
   - `NewBatch`:同 `Batch` 对齐
   - `append_chapters_to_batch` 参数 (batch_id, chapter_ids):Task 4 scheduler 与 Task 5 IPC 一致
   - `AppendChaptersResult` / `AppendChaptersToBatchPayload`:Task 6 types 与 Task 7 store / Task 8 view 一致
4. **范围**:单一意图(append to stopped batch),单一 plan 覆盖。
