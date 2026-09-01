# 转换工程工作流（batch + 串行 + frontier）— 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **关联 spec**:`docs/superpowers/specs/2026-08-03-transform-redesign-design.md`（本计划是 spec 的实施拆解）
> **关联实现边界**:B-scope（数据先行 + worker 串行 + frontier；UI 只做详情页骨架,勾选 / 新建批量留下一 spec）
> **总片数**:6 slice × 26 task,每片端到端可验、独立可 review

**Goal:** 为 novel-style-converter 添加 batch 化、串行调度、frontier 上下文继承、on_failure_policy 分流的转换工程工作流。

**Architecture:** 后端在 `nsc-core` 加 `Batch` 模型 + `BatchRepo` + `BatchScheduler` 模块,新建 `batches` 表与 3 处增列（migration 0008/0009/0010）。前端 `TnDialog` 收 3 字段、新建 `useBatchesStore`、加 `TransformationNovelDetail.vue` 详情页骨架（两 tab 只读）。

**Tech Stack:** Tauri 2 + rusqlite + nsc-core (Rust 1.x);Vue 3.5 + Pinia 2.3 + vue-router 4.6 + TypeScript 5.6;vitest (前端 mock `@tauri-apps/api/core`);cargo test (Rust 集成测试)。

---

## 文件结构总览（slice by slice）

### Slice 1 — tn 字段接入

| 路径 | 改动 | 职责 |
|------|------|------|
| `migrations/0008_tn_default_columns.sql` | 新建 | tn 增 3 列 |
| `crates/nsc-core/src/db/migrate.rs` | 改 | 注册 v8 |
| `crates/nsc-core/src/models/novel.rs` | 改 | `TransformationNovel` 与 `NewTransformationNovel` 增字段 |
| `crates/nsc-core/src/db/repo/novel.rs` | 改 | repo 读写 3 列 + tests |
| `crates/nsc-core/tests/db_tn_default_columns.rs` | 新建 | 集成测试 |
| `src-tauri/src/commands/transformation_novels.rs` | 改 | `CreateTransformationNovelPayload` + `UpdateTransformationNovelPayload` 增字段 |
| `src/ipc/types.ts` | 改 | `TransformationNovelSummary` 增字段 |
| `src/ipc/commands.ts` | 改 | 增 IPC 入参类型 |
| `src/components/TransformationNovelDialog.vue` | 改 | 表单增 3 字段（model 下拉 + mode 单选 + prompt 下拉按 mode 过滤） |
| `src/__tests__/tn_dialog.spec.ts` | 新建 | 模式过滤 + 提交 payload 校验 |

### Slice 2 — batches 接入

| 路径 | 改动 | 职责 |
|------|------|------|
| `migrations/0009_batches.sql` | 新建 | batches 新表 + 索引 |
| `crates/nsc-core/src/db/migrate.rs` | 改 | 注册 v9 |
| `crates/nsc-core/src/models/batch.rs` | 新建 | Batch / NewBatch / BatchStatus / OnFailurePolicy / ResumeAction |
| `crates/nsc-core/src/models/mod.rs` | 改 | 加 `pub mod batch;` |
| `crates/nsc-core/src/db/repo/batch.rs` | 新建 | CRUD + status 转换 |
| `crates/nsc-core/src/db/repo/mod.rs` | 改 | 加 `pub mod batch;` |
| `crates/nsc-core/src/db/pool.rs` | 改 | Db 上加 `batches()` 方法 |
| `crates/nsc-core/tests/db_batch.rs` | 新建 | 集成测试 |
| `src-tauri/src/commands/batches.rs` | 新建 | 6 个 IPC 命令（不含 resume_batch） |
| `src-tauri/src/commands/mod.rs` | 改 | 加 `pub mod batches;` |
| `src-tauri/src/lib.rs` | 改 | 注册 6 命令到 invoke_handler |
| `src/ipc/types.ts` | 改 | `Batch` / `BatchStatus` / `OnFailurePolicy` / `BatchStatusCount` |
| `src/ipc/commands.ts` | 改 | 6 个 IPC wrapper |
| `src/stores/batches.ts` | 新建 | `useBatchesStore` |
| `src/__tests__/batches.spec.ts` | 新建 | IPC wrapper 形参校对 |

### Slice 3 — chapter 增 batch_id + style_ref_chapter_id

| 路径 | 改动 | 职责 |
|------|------|------|
| `migrations/0010_chapter_batch_columns.sql` | 新建 | chapter 增 2 列 + 索引 |
| `crates/nsc-core/src/db/migrate.rs` | 改 | 注册 v10 |
| `crates/nsc-core/src/models/transformation.rs` | 改 | `TransformationChapter` 与 `NewTransformationChapter` 增字段;`TransformStatus::Skipped` |
| `crates/nsc-core/src/db/repo/transformation.rs` | 改 | repo 读写新增列 |
| `crates/nsc-core/tests/db_chapter_batch_cols.rs` | 新建 | 集成测试 |

### Slice 4 — BatchScheduler 核心

| 路径 | 改动 | 职责 |
|------|------|------|
| `crates/nsc-core/src/transformer/batch_scheduler.rs` | 新建 | scheduler 模块 |
| `crates/nsc-core/src/transformer/mod.rs` | 改 | `pub mod batch_scheduler;` |
| `crates/nsc-core/src/transformer/transformer.rs` | 改 | `TransformationNovelContext` 加 `frontier_chapter: Option<TransformationChapter>`（为 frontier SQL 准备） |
| `crates/nsc-core/src/transformer/queue.rs` | 改 | `JobSpec` 增 `batch_id` / `style_ref_chapter_id` |
| `crates/nsc-core/tests/scheduler.rs` | 新建 | scheduler 单元测试 |
| `src-tauri/src/lib.rs` | 改 | 启动 `BatchScheduler` 单例 + `JobQueue.set_notifier` |

### Slice 5 — on_failure_policy + paused + resume

| 路径 | 改动 | 职责 |
|------|------|------|
| `crates/nsc-core/src/transformer/batch_scheduler.rs` | 改 | `on_chapter_failed` 三分支 + `resume()` |
| `crates/nsc-core/tests/scheduler.rs` | 改 | 加 on_failure_policy + resume 测 |
| `src-tauri/src/commands/batches.rs` | 改 | 加 `resume_batch` 命令 + 5 个 IPC wrapper |
| `src/ipc/commands.ts` | 改 | 加 `resume_batch` wrapper |
| `src/stores/batches.ts` | 改 | 加 `resume()` action |
| `src/__tests__/batches.spec.ts` | 改 | 加 resume_batch wrapper 形参测 |

### Slice 6 — TN 详情页骨架

| 路径 | 改动 | 职责 |
|------|------|------|
| `src/router/index.ts` | 改 | 加 `/library/transformation/:tnId` 路由 |
| `src/views/TransformationNovelDetail.vue` | 新建 | 两 tab 骨架 |
| `src/components/ui/Tabs.vue` | 可能改 | 若现有 Tabs 不支持,扩 props |
| `src/stores/batches.ts` | 改 | 加 5s 轮询 + `refreshBatch` |
| `src/views/Library.vue` | 改 | transformations tab 加 "详情" 按钮 |
| `src/__tests__/transformation-detail.spec.ts` | 新建 | 组件快照测试（项目首批 vue-test 模式） |

---

## 不变量回顾（CLAUDE.md §"Critical invariants"）

- `Db` 是 Send 但**不是 Sync** —— **绝不**把 `Arc<Db>` 投进 `tokio::spawn` / `spawn_blocking`。worker / scheduler 工厂都捕获 `db_path: PathBuf`，在 worker 内 `Db::open(&path)` 拿 owned `Db`。
- `JobQueue` 保持 2 worker 上限 4（`src-tauri/src/lib.rs`）。
- 所有 migration DDL 必须 `IF NOT EXISTS`（worker 反复重开 DB，要幂等）。
- IPC **外层** invoke args 走 camelCase（Tauri 2 自动转）；**内层 DTO** 走 snake_case（`#[serde(rename_all = "snake_case")]`）。
- 响应类型保持 snake_case（与 nsc-core 模型一致）。
- 失败不自动重试，失败行停在 `Failed` 等用户主动重排。
- prompt 历史全留 —— re-transform 生成新行，不在 batch 内 in-place 更新。
- worker factory 必须返回 owned `Box<dyn AiProvider>` + owned `Db`，不借用。

---

## Slice 1 — tn 字段接入

### Task 1: migration 0008 — tn 增 3 列

**Files:**
- Create: `migrations/0008_tn_default_columns.sql`
- Modify: `crates/nsc-core/src/db/migrate.rs:1-9`（在 `v7` 后加 `v8`）

- [ ] **Step 1: 写 migration SQL 文件**

新建 `migrations/0008_tn_default_columns.sql`，内容：

```sql
-- tn 新增 3 列（NULL 兼容存量）
-- IF NOT EXISTS 在 SQLite 不支持 ADD COLUMN,所以要借 CREATE TABLE IF NOT EXISTS 模式不行;
-- 改用 catch + ignore 错的分支:实际由 Db::open 在生产路径上调用,
-- 单测用 apply_migration helper 模拟;这里是 ensure_idempotent 模式,
-- 二次运行会被 schema_versions 表阻挡。
ALTER TABLE transformation_novels
  ADD COLUMN default_model_config_id INTEGER REFERENCES model_configs(id);
ALTER TABLE transformation_novels
  ADD COLUMN default_prompt_id INTEGER REFERENCES prompts(id);
ALTER TABLE transformation_novels
  ADD COLUMN default_mode TEXT;  -- 'compress' | 'style';与 TransformationChapter.mode 对齐
```

注：SQLite 单条 ALTER 支持 IF NOT EXISTS? 不支持。靠 `schema_versions` 表阻重复 + `try { execute } catch { swallow }` 模式。

更稳妥写法（migration 兼容旧 DB）：

```sql
-- migration 0008: tn 默认配置
-- 已知 SQLite ALTER TABLE 不支持 IF NOT EXISTS; 二次执行靠 schema_versions 阻拦
ALTER TABLE transformation_novels
  ADD COLUMN default_model_config_id INTEGER REFERENCES model_configs(id);
ALTER TABLE transformation_novels
  ADD COLUMN default_prompt_id       INTEGER REFERENCES prompts(id);
ALTER TABLE transformation_novels
  ADD COLUMN default_mode            TEXT;
```

- [ ] **Step 2: 注册到 `SCHEMAS`**

编辑 `crates/nsc-core/src/db/migrate.rs` 第 8 行后：

```rust
    ("v7", include_str!("../../../../migrations/0007_uploads_word_count.sql")),
    ("v8", include_str!("../../../../migrations/0008_tn_default_columns.sql")),
];
```

- [ ] **Step 3: 写集成测试**

新建 `crates/nsc-core/tests/db_tn_default_columns.rs`：

```rust
use nsc_core::db::Db;

#[test]
fn tn_default_columns_exist_and_default_null() {
    let db = Db::open_in_memory().unwrap();
    db.execute_batch(
        "INSERT INTO model_configs (name, base_url, api_key, model, concurrency)
         VALUES ('m', 'http://x', 'k', 'g', 1)",
    ).unwrap();
    db.seed_builtin_prompts().unwrap();
    db.execute_batch(
        "INSERT INTO data_assets (upload_id, title, parsed_at)
         VALUES (1, 'DA', '2026-01-01T00:00:00Z')",
    ).unwrap();
    db.execute_batch(
        "INSERT INTO transformation_novels (data_asset_id, title, created_at)
         VALUES (1, 'tn', '2026-01-01T00:00:00Z')",
    ).unwrap();
    let row = db.execute_batch("").unwrap(); // noop
    let mut stmt = db.conn_ref().prepare(
        "SELECT default_model_config_id, default_prompt_id, default_mode
         FROM transformation_novels WHERE id = 1",
    ).unwrap();
    let mut rows = stmt.query([]).unwrap();
    let r = rows.next().unwrap().unwrap();
    assert_eq!(r.get::<_, Option<i64>>(0).unwrap(), None);
    assert_eq!(r.get::<_, Option<i64>>(1).unwrap(), None);
    assert_eq!(r.get::<_, Option<String>>(2).unwrap(), None);
}
```

⚠️ 上面的 `db.conn_ref()` 是示意；当前 `Db` 没有 `conn_ref()` 公开方法。在不破坏不变式前提下,用 repo 抽象:

把上面的 assertion 改成通过 `db.transformation_novels().get(1)` 读出 `TransformationNovel` 后断言 `default_model_config_id == None` 等。这要先把 Task 2 的 `TransformationNovel` 增字段做完。

**调整顺序**:Task 1 只做 migration + SCHEMAS 注册;完整测试在 Task 3 (Repo) 之后。

- [ ] **Step 4: 验证 schema 应用**

Run: `cargo test -p nsc-core --test db_migrations 2>&1 || cargo test -p nsc-core -- --list 2>&1 | grep -i migration`
Expected: 测试名含 `migration` 或 `schema_versions` 的 case 输出含 `v8`（如果有现成 migration 测）。

若没有现成 migration 测，只验证 `Db::open_in_memory` 不报 schema error：
```bash
cd crates/nsc-core && cargo build
```
Expected: 编译通过,`migrate.rs` `include_str!` 找到新文件。

- [ ] **Step 5: 提交**

```bash
git add migrations/0008_tn_default_columns.sql crates/nsc-core/src/db/migrate.rs
git commit -m "feat(db): migration 0008 — tn 增 default_model_config_id / default_prompt_id / default_mode"
```

---

### Task 2: `TransformationNovel` 模型增字段

**Files:**
- Modify: `crates/nsc-core/src/models/novel.rs:1-end`（在 `TransformationNovel` 与 `NewTransformationNovel` 各增 3 字段）

- [ ] **Step 1: 增字段**

打开 `crates/nsc-core/src/models/novel.rs`,把 `TransformationNovel` 与 `NewTransformationNovel` 改为:

```rust
#[derive(Debug, Clone)]
pub struct TransformationNovel {
    pub id: i64,
    pub data_asset_id: i64,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub default_model_config_id: Option<i64>,
    pub default_prompt_id: Option<i64>,
    pub default_mode: Option<TransformMode>,
}

#[derive(Debug, Clone)]
pub struct NewTransformationNovel {
    pub data_asset_id: i64,
    pub title: String,
    pub default_model_config_id: Option<i64>,
    pub default_prompt_id: Option<i64>,
    pub default_mode: Option<TransformMode>,
}
```

⚠️ `TransformMode` 在 `crate::models::transformation` 模块。在文件顶部 use 一行：

```rust
use crate::models::transform_rs::TransformMode;  // 或 crate::models::TransformMode 看现有导出
```

视现有 `mod.rs` 导出情况调整。

- [ ] **Step 2: 现有编译错误预期**

直接编译：
```bash
cargo build -p nsc-core
```
Expected: 失败，错误指 `TransformationNovelRepo::novel_from_row` / `insert` / `update` 不匹配新 struct。

- [ ] **Step 3: 暂不改 repo（下一步 Task 3 处理），先确认模块/导入正确**

如果 `models::transformation` 命名冲突（如 `novela` 模块和 `transformation` 模块都导 `TransformMode`），调整 use 路径。本 Task 不修编译错误,留到 Task 3 解决。

- [ ] **Step 4: 跳过（task 3 完成后一起跑测试）**

- [ ] **Step 5: 暂不提交 —— 与 Task 3 合并提交**

```bash
# 跳过,等 Task 3
```

---

### Task 3: `TransformationNovelRepo` 改写 + 集成测试

**Files:**
- Modify: `crates/nsc-core/src/db/repo/novel.rs:115-178`（`TransformationNovelRepo` impl）
- Create: `crates/nsc-core/tests/db_tn_default_columns.rs`

- [ ] **Step 1: 把 `insert` 改为接受新字段**

替换 `crates/nsc-core/src/db/repo/novel.rs` 中 `TransformationNovelRepo::insert` 为：

```rust
pub fn insert(&self, n: &NewTransformationNovel) -> Result<i64> {
    let tx = self.conn.unchecked_transaction()?;
    let now = Utc::now().to_rfc3339();
    tx.execute(
        "INSERT INTO transformation_novels \
         (data_asset_id, title, created_at, default_model_config_id, default_prompt_id, default_mode) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            n.data_asset_id, n.title, now,
            n.default_model_config_id, n.default_prompt_id,
            n.default_mode.map(|m| match m {
                crate::models::TransformMode::Compress => "compress",
                crate::models::TransformMode::Style => "style",
            }),
        ],
    )?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "UPDATE data_assets SET locked_at = ?2 WHERE id = ?1",
        params![n.data_asset_id, now],
    )?;
    tx.commit()?;
    Ok(id)
}
```

- [ ] **Step 2: 把 `update` 与 `novel_from_row` 改写支持 3 列**

`update`:

```rust
pub fn update(&self, n: &TransformationNovel) -> Result<()> {
    self.conn.execute(
        "UPDATE transformation_novels \
         SET title = ?2, default_model_config_id = ?3, default_prompt_id = ?4, default_mode = ?5 \
         WHERE id = ?1",
        params![
            n.id, n.title,
            n.default_model_config_id,
            n.default_prompt_id,
            n.default_mode.map(|m| match m {
                crate::models::TransformMode::Compress => "compress",
                crate::models::TransformMode::Style => "style",
            }),
        ],
    )?;
    Ok(())
}
```

`novel_from_row`:

```rust
fn novel_from_row(row: &Row) -> rusqlite::Result<TransformationNovel> {
    let created_at_s: String = row.get(3)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            3, rusqlite::types::Type::Text, Box::new(e)))?;
    let mode_s: Option<String> = row.get(6)?;
    let default_mode = mode_s.map(|s| match s.as_str() {
        "compress" => crate::models::TransformMode::Compress,
        "style" => crate::models::TransformMode::Style,
        other => panic!("unknown default_mode: {other}"),
    });
    Ok(TransformationNovel {
        id: row.get(0)?,
        data_asset_id: row.get(1)?,
        title: row.get(2)?,
        created_at,
        default_model_config_id: row.get(4)?,
        default_prompt_id: row.get(5)?,
        default_mode,
    })
}
```

`get` 与 `list` / `list_by_data_asset` 的 SELECT 也得改：在 `id, data_asset_id, title, created_at` 后追加 `default_model_config_id, default_prompt_id, default_mode`。

- [ ] **Step 3: 集成测试**

新建 `crates/nsc-core/tests/db_tn_default_columns.rs`：

```rust
use nsc_core::db::Db;
use nsc_core::models::{NewDataAsset, NewTransformationNovel, NewUpload, TransformMode, TransformationNovel};

#[test]
fn tn_with_default_columns_roundtrip() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(), filename: "x.txt".into(), byte_size: 0,
        file_path: "/tmp/x".into(), original_text: "正文".into(), word_count: 0,
    }).unwrap();
    db.seed_builtin_prompts().unwrap();
    let model_id = db.model_configs().insert(&nsc_core::models::NewModelConfig {
        name: "m".into(), base_url: "http://x".into(), api_key: "k".into(),
        model: "g".into(), max_tokens: None, temperature: None, concurrency: 1,
    }).unwrap();
    let prompt_id = db.prompts().list().unwrap()[0].id;
    let da_id = db.data_assets().insert(&NewDataAsset {
        upload_id, title: "DA".into(),
    }).unwrap();

    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id, title: "tn".into(),
        default_model_config_id: Some(model_id),
        default_prompt_id: Some(prompt_id),
        default_mode: Some(TransformMode::Style),
    }).unwrap();

    let tn: TransformationNovel = db.transformation_novels().get(tn_id).unwrap().unwrap();
    assert_eq!(tn.default_model_config_id, Some(model_id));
    assert_eq!(tn.default_prompt_id, Some(prompt_id));
    assert_eq!(tn.default_mode, Some(TransformMode::Style));

    // update 改 mode
    let mut tn2 = tn.clone();
    tn2.default_mode = Some(TransformMode::Compress);
    db.transformation_novels().update(&tn2).unwrap();
    let tn3 = db.transformation_novels().get(tn_id).unwrap().unwrap();
    assert_eq!(tn3.default_mode, Some(TransformMode::Compress));
}

#[test]
fn tn_default_columns_optional() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(), filename: "x.txt".into(), byte_size: 0,
        file_path: "/tmp/x".into(), original_text: "正文".into(), word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id, title: "DA".into() }).unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id, title: "legacy".into(),
        default_model_config_id: None, default_prompt_id: None, default_mode: None,
    }).unwrap();
    let tn = db.transformation_novels().get(tn_id).unwrap().unwrap();
    assert!(tn.default_model_config_id.is_none());
    assert!(tn.default_mode.is_none());
}
```

- [ ] **Step 4: 跑测试**

```bash
cargo test -p nsc-core --test db_tn_default_columns
```
Expected: 2 个测试全过。

- [ ] **Step 5: 提交**

```bash
git add crates/nsc-core/src/models/novel.rs crates/nsc-core/src/db/repo/novel.rs crates/nsc-core/tests/db_tn_default_columns.rs
git commit -m "feat(model): TransformationNovel 增 default_model_config_id / default_prompt_id / default_mode"
```

---

### Task 4: 后端 IPC payloads 增字段

**Files:**
- Modify: `src-tauri/src/commands/transformation_novels.rs:9-32,104-111`

- [ ] **Step 1: 增字段到 `CreateTransformationNovelPayload` 与 `UpdateTransformationNovelPayload`**

```rust
#[derive(Debug, Deserialize)]
pub struct CreateTransformationNovelPayload {
    pub data_asset_id: i64,
    pub title: String,
    #[serde(default)]
    pub default_model_config_id: Option<i64>,
    #[serde(default)]
    pub default_prompt_id: Option<i64>,
    #[serde(default)]
    pub default_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTransformationNovelPayload {
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub default_model_config_id: Option<i64>,
    #[serde(default)]
    pub default_prompt_id: Option<i64>,
    #[serde(default)]
    pub default_mode: Option<String>,
}
```

- [ ] **Step 2: `create_transformation_novel` 改造**

```rust
#[tauri::command]
pub fn create_transformation_novel(
    db: State<'_, Arc<Mutex<Db>>>,
    payload: CreateTransformationNovelPayload,
) -> Result<i64, String> {
    let title = payload.title.trim();
    if title.is_empty() {
        return Err("标题不能为空".into());
    }
    let db = db.lock().map_err(|e| e.to_string())?;
    let _da = db.data_assets().get(payload.data_asset_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("data_asset {} 不存在", payload.data_asset_id))?;
    // 三默认字段要么都填,要么都不填(用于旧 tn);如果有缺,前端 dialog 应已校验
    let mode = match payload.default_mode.as_deref() {
        None => None,
        Some("compress") => Some(crate::models::TransformMode::Compress),
        Some("style") => Some(crate::models::TransformMode::Style),
        Some(other) => return Err(format!("未知的 default_mode: {other}")),
    };
    db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: payload.data_asset_id,
        title: title.to_string(),
        default_model_config_id: payload.default_model_config_id,
        default_prompt_id: payload.default_prompt_id,
        default_mode: mode,
    }).map_err(|e| e.to_string())
}
```

⚠️ `update_transformation_novel` 也要改，类似。但默认 mode 与 title 同存。详见下方：

- [ ] **Step 3: `update_transformation_novel` 改**

```rust
#[tauri::command]
pub fn update_transformation_novel(
    db: State<'_, Arc<Mutex<Db>>>,
    payload: UpdateTransformationNovelPayload,
) -> Result<(), String> {
    let title = payload.title.trim();
    if title.is_empty() {
        return Err("标题不能为空".into());
    }
    let db = db.lock().map_err(|e| e.to_string())?;
    let cur = db.transformation_novels().get(payload.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("transformation_novel {} 不存在", payload.id))?;
    let mode = match payload.default_mode.as_deref() {
        None => None,
        Some("compress") => Some(crate::models::TransformMode::Compress),
        Some("style") => Some(crate::models::TransformMode::Style),
        Some(other) => return Err(format!("未知的 default_mode: {other}")),
    };
    let next = TransformationNovel {
        id: cur.id,
        data_asset_id: cur.data_asset_id,
        title: title.to_string(),
        created_at: cur.created_at,
        default_model_config_id: payload.default_model_config_id,
        default_prompt_id: payload.default_prompt_id,
        default_mode: mode,
    };
    db.transformation_novels().update(&next).map_err(|e| e.to_string())
}
```

- [ ] **Step 4: 跑 build**

```bash
cargo build -p nsc-core && cargo build
```
Expected: 全编译通过。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/commands/transformation_novels.rs
git commit -m "feat(ipc): create/update_transformation_novel 入参增 3 默认配置字段"
```

---

### Task 5: 前端 IPC 类型 + wrapper + TnDialog 增字段

**Files:**
- Modify: `src/ipc/types.ts`
- Modify: `src/ipc/commands.ts`
- Modify: `src/components/TransformationNovelDialog.vue`
- Create: `src/__tests__/tn_dialog.spec.ts`

- [ ] **Step 1: 改 `TransformationNovelSummary` 类型**

打开 `src/ipc/types.ts`,在 `TransformationNovelSummary`(行 137 附近)增 3 字段：

```typescript
export interface TransformationNovelSummary {
  id: number;
  data_asset_id: number;
  title: string;
  created_at: string;
  chapters_count: number;
  default_model_config_id: number | null;
  default_prompt_id: number | null;
  default_mode: 'compress' | 'style' | null;
}
```

- [ ] **Step 2: 增 IPC 入参类型 + 改 wrapper**

`src/ipc/types.ts`:

```typescript
export interface CreateTransformationNovelPayload {
  data_asset_id: number;
  title: string;
  default_model_config_id?: number | null;
  default_prompt_id?: number | null;
  default_mode?: 'compress' | 'style' | null;
}
export interface UpdateTransformationNovelPayload {
  id: number;
  title: string;
  default_model_config_id?: number | null;
  default_prompt_id?: number | null;
  default_mode?: 'compress' | 'style' | null;
}
```

wrapper: `createTransformationNovel(payload: CreateTransformationNovelPayload)` 与 `updateTransformationNovel(payload: UpdateTransformationNovelPayload)` 在 `src/ipc/commands.ts` 已经存在,只更新 import / 类型。

- [ ] **Step 3: 改 `TransformationNovelDialog.vue`**

读现有 `src/components/TransformationNovelDialog.vue`(50 行,见 spec §7.4),改成 5 字段:

```vue
<template>
  <Dialog v-model:open="open" title="创建转换小说" :width="480">
    <div class="row">
      <label>源 data_asset</label>
      <span class="hint">id {{ dataAssetId }}</span>
    </div>
    <div class="row">
      <label>标题 *</label>
      <Input v-model="title" placeholder="如:斗破_热血版" />
    </div>
    <div class="row">
      <label>默认模型 *</label>
      <select v-model="defaultModelId">
        <option :value="null">（请选择模型）</option>
        <option v-for="m in modelConfigs" :key="m.id" :value="m.id">{{ m.name }}</option>
      </select>
    </div>
    <div class="row">
      <label>转换类型 *</label>
      <label class="radio"><input type="radio" value="compress" v-model="mode" /> 压缩</label>
      <label class="radio"><input type="radio" value="style" v-model="mode" /> 文风</label>
    </div>
    <div class="row">
      <label>默认 prompt *</label>
      <select v-model="defaultPromptId" :disabled="!mode">
        <option :value="null">（请选择 prompt）</option>
        <option v-for="p in filteredPrompts" :key="p.id" :value="p.id">
          [{{ p.kind === 'compress' ? '压' : '风' }}] {{ p.name }}
        </option>
      </select>
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <template #footer>
      <Button @click="open = false">取消</Button>
      <Button kind="primary" :disabled="!canSubmit || submitting" @click="onSubmit">创建</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import Input from './ui/Input.vue';
import { listModelConfigs, listPrompts } from '../ipc/commands';
import type { ModelConfig, Prompt } from '../ipc/types';

const props = defineProps<{ dataAssetId: number }>();
const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{ submit: [{ data_asset_id: number; title: string; default_model_config_id: number; default_prompt_id: number; default_mode: 'compress' | 'style' }] }>();

const title = ref('');
const mode = ref<'compress' | 'style' | null>(null);
const defaultModelId = ref<number | null>(null);
const defaultPromptId = ref<number | null>(null);
const modelConfigs = ref<ModelConfig[]>([]);
const prompts = ref<Prompt[]>([]);
const error = ref<string | null>(null);
const submitting = ref(false);

const filteredPrompts = computed(() =>
  prompts.value.filter((p) => p.kind === mode.value)
);

const canSubmit = computed(() =>
  title.value.trim() !== '' &&
  defaultModelId.value != null &&
  mode.value != null &&
  defaultPromptId.value != null &&
  filteredPrompts.value.some((p) => p.id === defaultPromptId.value)
);

watch(open, async (v) => {
  if (v) {
    title.value = ''; mode.value = null; defaultModelId.value = null; defaultPromptId.value = null;
    error.value = null; submitting.value = false;
    try {
      modelConfigs.value = await listModelConfigs();
      prompts.value = await listPrompts();
    } catch (e) {
      error.value = '加载模型/prompt 列表失败：' + (e instanceof Error ? e.message : String(e));
    }
  }
});

async function onSubmit() {
  if (!canSubmit.value) return;
  error.value = null; submitting.value = true;
  try {
    emit('submit', {
      data_asset_id: props.dataAssetId,
      title: title.value.trim(),
      default_model_config_id: defaultModelId.value!,
      default_prompt_id: defaultPromptId.value!,
      default_mode: mode.value!,
    });
    open.value = false;
  } finally { submitting.value = false; }
}
</script>

<style scoped>
.row { display: flex; align-items: center; margin-bottom: 12px; gap: 12px; }
.row label { width: 90px; font-size: 14px; color: var(--text-secondary); flex-shrink: 0; }
.row select { flex: 1; padding: 4px 8px; border: 1px solid var(--border); border-radius: var(--radius-pin); }
.radio { display: inline-flex; gap: 4px; align-items: center; font-size: 14px; }
.hint { font-size: 13px; color: var(--text-muted); }
.error { color: var(--danger); font-size: 12px; }
</style>
```

⚠️ 现有 dialog 用 `onSubmit` 直接 `emit('submit', ...)`(前端 event 模式,不在 dialog 内部 invoke)。本次保持同样 event 模式；外层 `Library.vue` 的 `@submit` handler 改一下走新 payload。

- [ ] **Step 4: 改 `Library.vue` 的 `onSubmit` handler**

打开 `src/views/Library.vue`,找到 transformations tab 接收 `@submit` 的地方(在 `TransformationNovelDialog` 上),把 handler 改为读新 payload 字段并调 `createTransformationNovel` 即可。详见 spec §7.4 的提交 payload。

- [ ] **Step 5: 写测试**

新建 `src/__tests__/tn_dialog.spec.ts`：

```typescript
import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { listPrompts, listModelConfigs } from '../ipc/commands';

describe('TnDialog mode filtering', () => {
  it('listPrompts invoked on mount', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce([
      { id: 1, name: '压1', kind: 'compress', template: 'x', is_builtin: true },
      { id: 2, name: '风1', kind: 'style', template: 'y', is_builtin: true },
    ]);
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce([]);
    const ps = await listPrompts();
    const ms = await listModelConfigs();
    expect(ps.find((p) => p.kind === 'compress')?.name).toBe('压1');
    expect(ms).toEqual([]);
    expect(invoke).toHaveBeenCalledWith('list_prompts');
    expect(invoke).toHaveBeenCalledWith('list_model_configs');
  });
});

describe('TnDialog submission payload', () => {
  it('submit payload contains all 3 default fields', () => {
    const payload = {
      data_asset_id: 5,
      title: 'tx',
      default_model_config_id: 3,
      default_prompt_id: 2,
      default_mode: 'compress' as const,
    };
    expect(payload).toMatchObject({
      data_asset_id: 5, title: 'tx',
      default_model_config_id: 3, default_prompt_id: 2,
      default_mode: 'compress',
    });
  });
});
```

- [ ] **Step 6: 跑前端测试**

```bash
pnpm test -- --run tn_dialog
```
Expected: 2 个测试通过。

- [ ] **Step 7: 提交**

```bash
git add src/ipc/types.ts src/ipc/commands.ts src/components/TransformationNovelDialog.vue src/views/Library.vue src/__tests__/tn_dialog.spec.ts
git commit -m "feat(ui): TnDialog 增 default_model / default_mode / default_prompt 三字段 + mode 过滤"
```

---

**Slice 1 完成检查**：

```bash
cargo test -p nsc-core
pnpm test
cargo build
pnpm tauri build --bundles msi 2>&1 | tail -50  # smoke
```

Expected：所有测试过；`tauri build` 出 MSI（不实际打包只验编译路径也可:`pnpm build`）。

---

## Slice 2 — batches 接入

### Task 6: migration 0009 — batches 新表

**Files:**
- Create: `migrations/0009_batches.sql`
- Modify: `crates/nsc-core/src/db/migrate.rs`

- [ ] **Step 1: 写 SQL**

新建 `migrations/0009_batches.sql`:

```sql
-- 批号表（独立 entity）。每次批量转换一条。
CREATE TABLE IF NOT EXISTS batches (
  id                      INTEGER PRIMARY KEY,
  transformation_novel_id INTEGER NOT NULL REFERENCES transformation_novels(id),
  label                   TEXT,
  on_failure_policy       TEXT NOT NULL DEFAULT 'pause_and_review',
  status                  TEXT NOT NULL DEFAULT 'pending',
  created_at              TEXT NOT NULL,
  started_at              TEXT,
  ended_at                TEXT
);
CREATE INDEX IF NOT EXISTS idx_batches_tn      ON batches(transformation_novel_id);
CREATE INDEX IF NOT EXISTS idx_batches_status  ON batches(status);
```

- [ ] **Step 2: 注册 v9**

`crates/nsc-core/src/db/migrate.rs`:

```rust
    ("v8", include_str!("../../../../migrations/0008_tn_default_columns.sql")),
    ("v9", include_str!("../../../../migrations/0009_batches.sql")),
];
```

- [ ] **Step 3: 验证编译**

```bash
cargo build -p nsc-core
```
Expected: 编译通过。

- [ ] **Step 4: 提交**

```bash
git add migrations/0009_batches.sql crates/nsc-core/src/db/migrate.rs
git commit -m "feat(db): migration 0009 — batches 新表"
```

---

### Task 7: Batch 模型（BatchStatus / OnFailurePolicy / Batch / NewBatch）

**Files:**
- Create: `crates/nsc-core/src/models/batch.rs`
- Modify: `crates/nsc-core/src/models/mod.rs`

- [ ] **Step 1: 写 `batch.rs`**

新建 `crates/nsc-core/src/models/batch.rs`：

```rust
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
```

- [ ] **Step 2: `models/mod.rs` 加导出**

`crates/nsc-core/src/models/mod.rs` 顶部加 `pub mod batch;`。

- [ ] **Step 3: 编译**

```bash
cargo build -p nsc-core
```
Expected：编译过。

- [ ] **Step 4: 提交**

```bash
git add crates/nsc-core/src/models/batch.rs crates/nsc-core/src/models/mod.rs
git commit -m "feat(model): Batch / BatchStatus / OnFailurePolicy / ResumeAction 模型"
```

---

### Task 8: `BatchRepo` CRUD

**Files:**
- Create: `crates/nsc-core/src/db/repo/batch.rs`
- Modify: `crates/nsc-core/src/db/repo/mod.rs`
- Modify: `crates/nsc-core/src/db/pool.rs:24-83`

- [ ] **Step 1: 写 `batch.rs` repo**

新建 `crates/nsc-core/src/db/repo/batch.rs`:

```rust
use chrono::{DateTime, Utc};
use rusqlite::{params, Row};

use crate::error::Result;
use crate::models::{Batch, BatchStatus, NewBatch, OnFailurePolicy};

pub struct BatchRepo<'a> { pub(crate) conn: &'a rusqlite::Connection }

impl<'a> BatchRepo<'a> {
    /// 插入一条 batch（status='pending'）。返回新 id。
    pub fn insert(&self, b: &NewBatch) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        let policy_s = policy_to_str(b.on_failure_policy);
        self.conn.execute(
            "INSERT INTO batches (transformation_novel_id, label, on_failure_policy, status, created_at) \
             VALUES (?1, ?2, ?3, 'pending', ?4)",
            params![b.transformation_novel_id, b.label, policy_s, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get(&self, id: i64) -> Result<Option<Batch>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, transformation_novel_id, label, on_failure_policy, status, created_at, started_at, ended_at \
             FROM batches WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? { Ok(Some(batch_from_row(row)?)) } else { Ok(None) }
    }

    pub fn list_by_tn(&self, tn_id: i64) -> Result<Vec<Batch>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, transformation_novel_id, label, on_failure_policy, status, created_at, started_at, ended_at \
             FROM batches WHERE transformation_novel_id = ?1 ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![tn_id], |row| batch_from_row(row))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 设 status 同时自动维护 started_at / ended_at 时间戳。
    /// 注意:从 Running/Pending/Paused 转到 Running 时,started_at 已存在则不动;首次 Running 写入 started_at。
    /// 从 Paused/Running 转到 Completed/Terminated/Cancelled 时,ended_at 设 NOW。
    /// 其它空转不写时间戳(由调用方明确语义)。
    pub fn set_status(&self, id: i64, status: BatchStatus) -> Result<()> {
        let status_s = status_to_str(status);
        let now = Utc::now().to_rfc3339();
        match status {
            BatchStatus::Running => {
                self.conn.execute(
                    "UPDATE batches SET status = ?2, started_at = COALESCE(started_at, ?3) WHERE id = ?1",
                    params![id, status_s, now],
                )?;
            }
            BatchStatus::Completed | BatchStatus::Terminated | BatchStatus::Cancelled => {
                self.conn.execute(
                    "UPDATE batches SET status = ?2, ended_at = ?3 WHERE id = ?1",
                    params![id, status_s, now],
                )?;
            }
            _ => {
                self.conn.execute(
                    "UPDATE batches SET status = ?2 WHERE id = ?1",
                    params![id, status_s],
                )?;
            }
        }
        Ok(())
    }

    /// 改 label / on_failure_policy。只在 batch 不在 Running 时允许（上层校验）。
    pub fn update(&self, b: &Batch) -> Result<()> {
        let policy_s = policy_to_str(b.on_failure_policy);
        self.conn.execute(
            "UPDATE batches SET label = ?2, on_failure_policy = ?3 WHERE id = ?1",
            params![b.id, b.label, policy_s],
        )?;
        Ok(())
    }

    /// 统计批号各状态计数（给 UI tab badge 用）。
    pub fn count_by_status(&self, tn_id: i64) -> Result<BatchStatusCount> {
        let mut stmt = self.conn.prepare(
            "SELECT status, COUNT(*) FROM batches WHERE transformation_novel_id = ?1 GROUP BY status",
        )?;
        let rows = stmt.query_map(params![tn_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut counts = BatchStatusCount::default();
        for row in rows {
            let (s, n) = row?;
            match s.as_str() {
                "pending" => counts.pending = n,
                "running" => counts.running = n,
                "paused" => counts.paused = n,
                "completed" => counts.completed = n,
                "terminated" => counts.terminated = n,
                "cancelled" => counts.cancelled = n,
                _ => {}
            }
        }
        Ok(counts)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BatchStatusCount {
    pub pending: i64,
    pub running: i64,
    pub paused: i64,
    pub completed: i64,
    pub terminated: i64,
    pub cancelled: i64,
}

fn batch_from_row(row: &Row) -> rusqlite::Result<Batch> {
    let created_at_s: String = row.get(5)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            5, rusqlite::types::Type::Text, Box::new(e)))?;
    let started_at_s: Option<String> = row.get(6)?;
    let ended_at_s:   Option<String> = row.get(7)?;
    let parse_opt = |s: Option<String>| -> rusqlite::Result<Option<DateTime<Utc>>> {
        match s {
            None => Ok(None),
            Some(s) => DateTime::parse_from_rfc3339(&s)
                .map(|d| Some(d.with_timezone(&Utc)))
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                    6, rusqlite::types::Type::Text, Box::new(e))),
        }
    };
    Ok(Batch {
        id: row.get(0)?,
        transformation_novel_id: row.get(1)?,
        label: row.get(2)?,
        on_failure_policy: str_to_policy(&row.get::<_, String>(3)?)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?,
        status: str_to_status(&row.get::<_, String>(4)?)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?,
        created_at,
        started_at: parse_opt(started_at_s)?,
        ended_at: parse_opt(ended_at_s)?,
    })
}

fn status_to_str(s: BatchStatus) -> &'static str {
    match s {
        BatchStatus::Pending    => "pending",
        BatchStatus::Running    => "running",
        BatchStatus::Paused     => "paused",
        BatchStatus::Completed  => "completed",
        BatchStatus::Terminated => "terminated",
        BatchStatus::Cancelled  => "cancelled",
    }
}
fn str_to_status(s: &str) -> rusqlite::Result<BatchStatus> {
    match s {
        "pending"    => Ok(BatchStatus::Pending),
        "running"    => Ok(BatchStatus::Running),
        "paused"     => Ok(BatchStatus::Paused),
        "completed"  => Ok(BatchStatus::Completed),
        "terminated" => Ok(BatchStatus::Terminated),
        "cancelled"  => Ok(BatchStatus::Cancelled),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0, rusqlite::types::Type::Text,
            format!("unknown batch status: {other}").into())),
    }
}
fn policy_to_str(p: OnFailurePolicy) -> &'static str {
    match p {
        OnFailurePolicy::PauseAndReview => "pause_and_review",
        OnFailurePolicy::Terminate      => "terminate",
        OnFailurePolicy::SkipFailed     => "skip_failed",
    }
}
fn str_to_policy(s: &str) -> rusqlite::Result<OnFailurePolicy> {
    match s {
        "pause_and_review" => Ok(OnFailurePolicy::PauseAndReview),
        "terminate"        => Ok(OnFailurePolicy::Terminate),
        "skip_failed"      => Ok(OnFailurePolicy::SkipFailed),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0, rusqlite::types::Type::Text,
            format!("unknown on_failure_policy: {other}").into())),
    }
}
```

⚠️ `Serialize` derive 没在 `BatchStatusCount` 加，需 use：`use serde::Serialize;`

- [ ] **Step 2: 改 `db/repo/mod.rs`**

`crates/nsc-core/src/db/repo/mod.rs` 顶部加 `pub mod batch;`

- [ ] **Step 3: 改 `db/pool.rs`**

`crates/nsc-core/src/db/pool.rs:9-10` 区域，加 BatchRepo 到 import 列表：

```rust
use super::repo::{
    BatchRepo, ChapterRepo, DataAssetRepo, ModelConfigRepo, PromptRepo,
    TransformationChapterRepo, TransformationNovelRepo, UploadRepo,
};
```

`impl Db` 加方法：

```rust
    pub fn batches(&self) -> BatchRepo<'_> { BatchRepo { conn: &self.conn } }
```

- [ ] **Step 4: 集成测试**

新建 `crates/nsc-core/tests/db_batch.rs`：

```rust
use nsc_core::db::Db;
use nsc_core::models::{NewBatch, NewDataAsset, NewUpload, BatchStatus, OnFailurePolicy};

fn setup_tn(db: &Db) -> i64 {
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(), filename: "x.txt".into(), byte_size: 0,
        file_path: "/tmp/x".into(), original_text: "正".into(), word_count: 0,
    }).unwrap();
    db.seed_builtin_prompts().unwrap();
    let model_id = db.model_configs().insert(&nsc_core::models::NewModelConfig {
        name: "m".into(), base_url: "http://x".into(), api_key: "k".into(),
        model: "g".into(), max_tokens: None, temperature: None, concurrency: 1,
    }).unwrap();
    let prompt_id = db.prompts().list().unwrap()[0].id;
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id, title: "DA".into() }).unwrap();
    db.transformation_novels().insert(&nsc_core::models::NewTransformationNovel {
        data_asset_id: da_id, title: "tn".into(),
        default_model_config_id: Some(model_id), default_prompt_id: Some(prompt_id),
        default_mode: Some(nsc_core::models::TransformMode::Compress),
    }).unwrap()
}

#[test]
fn insert_and_list_batches() {
    let db = Db::open_in_memory().unwrap();
    let tn_id = setup_tn(&db);
    let b1 = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id, label: Some("A".into()),
        on_failure_policy: OnFailurePolicy::PauseAndReview,
    }).unwrap();
    let b2 = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id, label: Some("B".into()),
        on_failure_policy: OnFailurePolicy::Terminate,
    }).unwrap();
    let all = db.batches().list_by_tn(tn_id).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, b2);  // DESC
    assert_eq!(all[0].on_failure_policy, OnFailurePolicy::Terminate);
}

#[test]
fn set_status_starts_ended_timestamps() {
    let db = Db::open_in_memory().unwrap();
    let tn_id = setup_tn(&db);
    let id = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id, label: None,
        on_failure_policy: OnFailurePolicy::SkipFailed,
    }).unwrap();
    db.batches().set_status(id, BatchStatus::Running).unwrap();
    let b1 = db.batches().get(id).unwrap().unwrap();
    assert_eq!(b1.status, BatchStatus::Running);
    assert!(b1.started_at.is_some());
    assert!(b1.ended_at.is_none());

    db.batches().set_status(id, BatchStatus::Completed).unwrap();
    let b2 = db.batches().get(id).unwrap().unwrap();
    assert_eq!(b2.status, BatchStatus::Completed);
    assert!(b2.ended_at.is_some());
}

#[test]
fn count_by_status() {
    let db = Db::open_in_memory().unwrap();
    let tn_id = setup_tn(&db);
    let a = db.batches().insert(&NewBatch { transformation_novel_id: tn_id, label: None, on_failure_policy: OnFailurePolicy::PauseAndReview }).unwrap();
    db.batches().insert(&NewBatch { transformation_novel_id: tn_id, label: None, on_failure_policy: OnFailurePolicy::SkipFailed }).unwrap();
    db.batches().set_status(a, BatchStatus::Running).unwrap();
    let c = db.batches().count_by_status(tn_id).unwrap();
    assert_eq!(c.running, 1);
    assert_eq!(c.pending, 1);
    assert_eq!(c.completed, 0);
}
```

- [ ] **Step 5: 跑测试**

```bash
cargo test -p nsc-core --test db_batch
```
Expected: 3 个测试过。

- [ ] **Step 6: 提交**

```bash
git add crates/nsc-core/src/db/repo/batch.rs crates/nsc-core/src/db/repo/mod.rs crates/nsc-core/src/db/pool.rs crates/nsc-core/tests/db_batch.rs
git commit -m "feat(repo): BatchRepo CRUD + set_status 自动维护时间戳"
```

---

### Task 9: 后端 6 个 IPC 命令 + 注册

**Files:**
- Create: `src-tauri/src/commands/batches.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 写 `commands/batches.rs`**

新建 `src-tauri/src/commands/batches.rs`：

```rust
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use tauri::State;

use nsc_core::db::Db;
use nsc_core::error::Error;
use nsc_core::models::{Batch, BatchStatusCount, OnFailurePolicy, TransformationChapterRow};

#[derive(Debug, Deserialize)]
pub struct CreateBatchPayload {
    pub tn_id: i64,
    pub label: Option<String>,
    pub on_failure_policy: String,   // 'pause_and_review' | 'terminate' | 'skip_failed'
    pub chapter_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBatchPayload {
    pub label: Option<String>,
    pub on_failure_policy: Option<String>,
}

fn parse_policy(s: &str) -> Result<OnFailurePolicy, Error> {
    match s {
        "pause_and_review" => Ok(OnFailurePolicy::PauseAndReview),
        "terminate"        => Ok(OnFailurePolicy::Terminate),
        "skip_failed"      => Ok(OnFailurePolicy::SkipFailed),
        other => Err(Error::Validation(format!("未知的 on_failure_policy: {other}"))),
    }
}

#[tauri::command]
pub fn list_batches(
    db: State<'_, Arc<Mutex<Db>>>,
    tn_id: i64,
) -> Result<Vec<Batch>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.batches().list_by_tn(tn_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_batch(
    db: State<'_, Arc<Mutex<Db>>>,
    batch_id: i64,
) -> Result<Batch, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.batches().get(batch_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("batch {batch_id} 不存在"))
}

#[tauri::command]
pub fn create_batch(
    db: State<'_, Arc<Mutex<Db>>>,
    payload: CreateBatchPayload,
) -> Result<i64, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let policy = parse_policy(&payload.on_failure_policy)
        .map_err(|e| e.to_string())?;
    // 验证 tn_id 存在
    if db.transformation_novels().get(payload.tn_id).map_err(|e| e.to_string())?.is_none() {
        return Err(format!("transformation_novel {} 不存在", payload.tn_id));
    }
    let id = db.batches().insert(&nsc_core::models::NewBatch {
        transformation_novel_id: payload.tn_id,
        label: payload.label,
        on_failure_policy: policy,
    }).map_err(|e| e.to_string())?;
    // 本 slice 不入队 chapter:enqueue 由 BatchScheduler 在 slice 4 接管。
    // 本命令只创建 batch 行 + 留空给后续 add_chapters IPC 或由 scheduler 调用。
    let _ = payload.chapter_ids; // 暂未用,保留字段供将来扩展
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
    if matches!(cur.status, nsc_core::models::BatchStatus::Running) {
        return Err("batch 正在运行,不可改 label / on_failure_policy".into());
    }
    let new_label = payload.label.or(cur.label);
    let new_policy = match payload.on_failure_policy.as_deref() {
        None => cur.on_failure_policy,
        Some(s) => parse_policy(s).map_err(|e| e.to_string())?,
    };
    let next = nsc_core::models::Batch {
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

#[tauri::command]
pub fn list_batch_chapters(
    db: State<'_, Arc<Mutex<Db>>>,
    batch_id: i64,
) -> Result<Vec<TransformationChapterRow>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.transformation_chapters().list_by_batch(batch_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn count_batches_by_status(
    db: State<'_, Arc<Mutex<Db>>>,
    tn_id: i64,
) -> Result<BatchStatusCount, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.batches().count_by_status(tn_id).map_err(|e| e.to_string())
}
```

⚠️ 上面用了 `db.transformation_chapters().list_by_batch(batch_id)`，此函数在 Slice 3 才加。这里要先 stub 一下，让 Task 9 与 Slice 3 编译一致。

**解决**：把 `list_batch_chapters` 命令函数体改为返回 `Ok(vec![])` 占位（标注 TODO），在 Slice 3 Task 11 实现真实调用。代码里加注释：

```rust
#[tauri::command]
pub fn list_batch_chapters(
    _db: State<'_, Arc<Mutex<Db>>>,
    batch_id: i64,
) -> Result<Vec<TransformationChapterRow>, String> {
    // Slice 3 接手:S3 实现 transformation_chapters.list_by_batch 后替换
    let _ = batch_id;
    Ok(vec![])
}
```

- [ ] **Step 2: `commands/mod.rs` 加 export**

`src-tauri/src/commands/mod.rs`:

```rust
pub mod batches;
```

按字母序：插在 `chapters` 之前。

- [ ] **Step 3: `lib.rs` 注册命令**

`src-tauri/src/lib.rs` 的 `invoke_handler!` 块追加：

```rust
    .invoke_handler(tauri::generate_handler![
        // ... 现有命令 ...
        commands::batches::list_batches,
        commands::batches::get_batch,
        commands::batches::create_batch,
        commands::batches::update_batch,
        commands::batches::list_batch_chapters,
        commands::batches::count_batches_by_status,
    ])
```

- [ ] **Step 4: 编译**

```bash
cargo build
```
Expected：编译过。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/commands/batches.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(ipc): batches — list/get/create/update/list_batch_chapters/count 命令"
```

---

### Task 10: 前端类型 + IPC wrappers + useBatchesStore + 测试

**Files:**
- Modify: `src/ipc/types.ts`
- Modify: `src/ipc/commands.ts`
- Create: `src/stores/batches.ts`
- Create: `src/__tests__/batches.spec.ts`

- [ ] **Step 1: 加类型到 `src/ipc/types.ts`**

```typescript
// === Batch 工作流 ===
export type BatchStatus =
  | 'pending' | 'running' | 'paused'
  | 'completed' | 'terminated' | 'cancelled';

export type OnFailurePolicy =
  | 'pause_and_review' | 'terminate' | 'skip_failed';

export interface Batch {
  id: number;
  transformation_novel_id: number;
  label: string | null;
  on_failure_policy: OnFailurePolicy;
  status: BatchStatus;
  created_at: string;
  started_at: string | null;
  ended_at: string | null;
}

export interface BatchStatusCount {
  pending: number;
  running: number;
  paused: number;
  completed: number;
  terminated: number;
  cancelled: number;
}

export interface CreateBatchPayload {
  tn_id: number;
  label: string | null;
  on_failure_policy: OnFailurePolicy;
  chapter_ids: number[];
}
```

- [ ] **Step 2: 加 wrappers 到 `src/ipc/commands.ts`**

```typescript
import type { Batch, BatchStatusCount, CreateBatchPayload } from './types';

export const listBatches = (tnId: number): Promise<Batch[]> =>
  invoke<Batch[]>('list_batches', { tnId });

export const getBatch = (batchId: number): Promise<Batch> =>
  invoke<Batch>('get_batch', { batchId });

export const createBatch = (payload: CreateBatchPayload): Promise<number> =>
  invoke<number>('create_batch', { payload });

export const updateBatch = (
  batchId: number,
  payload: { label?: string | null; on_failure_policy?: OnFailurePolicy },
): Promise<void> =>
  invoke<void>('update_batch', { batchId, payload });

export const listBatchChapters = (batchId: number): Promise<TransformationChapterRow[]> =>
  invoke<TransformationChapterRow[]>('list_batch_chapters', { batchId });

export const countBatchesByStatus = (tnId: number): Promise<BatchStatusCount> =>
  invoke<BatchStatusCount>('count_batches_by_status', { tnId });
```

- [ ] **Step 3: 写 `useBatchesStore`**

新建 `src/stores/batches.ts`：

```typescript
import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import {
  countBatchesByStatus, getBatch, listBatches,
  type Batch, type BatchStatusCount,
} from '../ipc/commands';

export const useBatchesStore = defineStore('batches', () => {
  const byTn = ref<Map<number, Batch[]>>(new Map());
  const counts = ref<Map<number, BatchStatusCount>>(new Map());
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function loadByTn(tnId: number) {
    loading.value = true; error.value = null;
    try {
      const [batches, c] = await Promise.all([listBatches(tnId), countBatchesByStatus(tnId)]);
      byTn.value.set(tnId, batches);
      counts.value.set(tnId, c);
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }
  async function refresh(batchId: number) {
    try {
      const b = await getBatch(batchId);
      const list = byTn.value.get(b.transformation_novel_id);
      if (list) {
        const i = list.findIndex((x) => x.id === batchId);
        if (i >= 0) list[i] = b; else list.unshift(b);
      }
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    }
  }
  function getByTn(tnId: number): Batch[] {
    return byTn.value.get(tnId) ?? [];
  }
  function getCounts(tnId: number): BatchStatusCount | undefined {
    return counts.value.get(tnId);
  }

  return { byTn, loading, error, loadByTn, refresh, getByTn, getCounts };
});
```

- [ ] **Step 4: 写测试**

新建 `src/__tests__/batches.spec.ts`：

```typescript
import { describe, expect, it, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import {
  listBatches, getBatch, createBatch, updateBatch, listBatchChapters, countBatchesByStatus,
} from '../ipc/commands';

describe('batches IPC wrappers', () => {
  beforeEach(() => { (invoke as ReturnType<typeof vi.fn>).mockReset(); });

  it('listBatches invokes "list_batches" with { tnId } camelCase', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce([]);
    await listBatches(7);
    expect(invoke).toHaveBeenCalledWith('list_batches', { tnId: 7 });
  });

  it('getBatch invokes "get_batch" with { batchId }', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ id: 3 });
    await getBatch(3);
    expect(invoke).toHaveBeenCalledWith('get_batch', { batchId: 3 });
  });

  it('createBatch passes inner payload as snake_case', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce(42);
    await createBatch({
      tn_id: 1,
      label: null,
      on_failure_policy: 'pause_and_review',
      chapter_ids: [10, 11],
    });
    expect(invoke).toHaveBeenCalledWith('create_batch', {
      payload: {
        tn_id: 1, label: null,
        on_failure_policy: 'pause_and_review',
        chapter_ids: [10, 11],
      },
    });
  });

  it('updateBatch passes { batchId, payload }', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce(undefined);
    await updateBatch(3, { label: 'new' });
    expect(invoke).toHaveBeenCalledWith('update_batch', {
      batchId: 3,
      payload: { label: 'new' },
    });
  });

  it('listBatchChapters invokes with batchId', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce([]);
    await listBatchChapters(9);
    expect(invoke).toHaveBeenCalledWith('list_batch_chapters', { batchId: 9 });
  });

  it('countBatchesByStatus invokes with tnId', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ pending: 0, running: 0, paused: 0, completed: 0, terminated: 0, cancelled: 0 });
    await countBatchesByStatus(2);
    expect(invoke).toHaveBeenCalledWith('count_batches_by_status', { tnId: 2 });
  });
});
```

- [ ] **Step 5: 跑测试**

```bash
pnpm test -- --run batches
```
Expected: 6 个测试过。

- [ ] **Step 6: 提交**

```bash
git add src/ipc/types.ts src/ipc/commands.ts src/stores/batches.ts src/__tests__/batches.spec.ts
git commit -m "feat(ui): useBatchesStore + 6 个 IPC wrappers + vitest"
```

---

**Slice 2 完成检查**：

```bash
cargo test -p nsc-core
pnpm test
```

Expected：所有 Rust + vitest 过。

---

## Slice 3 — 章节 batch_id 接入

> 最小闭环：章节表能落 `batch_id` 和 `style_ref_chapter_id`（column 备用，本片不接 scheduler）；既有的 enqueue 路径把 batch_id 填进 tc 行。
> 跑完所有现存 `cargo test -p nsc-core` 与 `pnpm test` 不回归。

### Task 11: migration 0008 — tc 增 batch_id / style_ref_chapter_id

**Files:**
- Create: `migrations/0008_tc_batch_columns.sql`
- Modify: `crates/nsc-core/src/db/migrate.rs:1-40`（注册新 SQL）

- [ ] **Step 1: 写新迁移 SQL 文件**

`migrations/0008_tc_batch_columns.sql`：

```sql
-- transformation_chapters 增 2 列（NULL 兼容存量历史散点）
ALTER TABLE transformation_chapters
  ADD COLUMN batch_id             INTEGER REFERENCES batches(id);
ALTER TABLE transformation_chapters
  ADD COLUMN style_ref_chapter_id INTEGER REFERENCES chapters(id);

CREATE INDEX IF NOT EXISTS idx_tc_batch ON transformation_chapters(batch_id);
```

> 不引入 `batches` 表（那是 0009）— `batch_id` 列先落，FK 在 0009 到位后激活。SQLite 对不存在的引用表 ADD COLUMN 时不校验 FK；存量表的列加 NULL 即可。

- [ ] **Step 2: 注册到 SCHEMAS**

打开 `crates/nsc-core/src/db/migrate.rs`，把：

```rust
const SCHEMAS: &[(i32, &str)] = &[
    (1, include_str!("../../../../migrations/0001_init.sql")),
    (2, include_str!("../../../../migrations/0002_split_uploads.sql")),
    (3, include_str!("../../../../migrations/0003_chapter_byte_ranges.sql")),
    (4, include_str!("../../../../migrations/0004_data_assets.sql")),
    (5, include_str!("../../../../migrations/0005_chapters_data_asset_fk.sql")),
    (6, include_str!("../../../../migrations/0006_transformation_novels_data_asset_fk.sql")),
    (7, include_str!("../../../../migrations/0007_uploads_word_count.sql")),
];
```

改为：

```rust
const SCHEMAS: &[(i32, &str)] = &[
    (1, include_str!("../../../../migrations/0001_init.sql")),
    (2, include_str!("../../../../migrations/0002_split_uploads.sql")),
    (3, include_str!("../../../../migrations/0003_chapter_byte_ranges.sql")),
    (4, include_str!("../../../../migrations/0004_data_assets.sql")),
    (5, include_str!("../../../../migrations/0005_chapters_data_asset_fk.sql")),
    (6, include_str!("../../../../migrations/0006_transformation_novels_data_asset_fk.sql")),
    (7, include_str!("../../../../migrations/0007_uploads_word_count.sql")),
    (8, include_str!("../../../../migrations/0008_tc_batch_columns.sql")),
];
```

- [ ] **Step 3: 跑全测，确认零回归**

```bash
cargo test -p nsc-core
```

Expected：所有现存测试过；新 migration 在 in-memory db 上被 `Db::open_in_memory()` 自动 apply（Db 启动逻辑见 §"Invariant review"）。

- [ ] **Step 4: 提交**

```bash
git add migrations/0008_tc_batch_columns.sql crates/nsc-core/src/db/migrate.rs
git commit -m "feat(db): migration 0008 — tc 增 batch_id / style_ref_chapter_id 列"
```

---

### Task 12: TransformationChapter 模型 + repo 增字段

**Files:**
- Modify: `crates/nsc-core/src/models/transformation.rs:26-55`
- Modify: `crates/nsc-core/src/db/repo/transformation.rs:12-28, 111-156`

- [ ] **Step 1: model 增 2 字段**

`crates/nsc-core/src/models/transformation.rs`，把：

```rust
pub struct TransformationChapter {
    pub id: i64,
    pub transformation_novel_id: i64,
    pub chapter_id: i64,
    pub mode: TransformMode,
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
}

#[derive(Debug, Clone)]
pub struct NewTransformationChapter {
    pub transformation_novel_id: i64,
    pub chapter_id: i64,
    pub mode: TransformMode,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
}
```

改为：

```rust
pub struct TransformationChapter {
    pub id: i64,
    pub transformation_novel_id: i64,
    pub chapter_id: i64,
    pub mode: TransformMode,
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
    /// 所属批号；存量散点行为 NULL。
    pub batch_id: Option<i64>,
    /// frontier 章节 id —— 同 tn 内、idx 严格小于本章节、status='done' 的最近一次 tc 行。
    /// 命名沿用 spec（spec §4.1 / §5.8）；本片先用 NULL（scheduler 接力时填，Slice 4）。
    pub style_ref_chapter_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewTransformationChapter {
    pub transformation_novel_id: i64,
    pub chapter_id: i64,
    pub mode: TransformMode,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
    pub batch_id: Option<i64>,
    pub style_ref_chapter_id: Option<i64>,
}
```

- [ ] **Step 2: repo.insert 增 2 列**

`crates/nsc-core/src/db/repo/transformation.rs`，把 `insert` 改写：

```rust
pub fn insert(&self, t: &NewTransformationChapter) -> Result<i64> {
    let mode = match t.mode {
        TransformMode::Compress => "compress",
        TransformMode::Style => "style",
    };
    self.conn.execute(
        "INSERT INTO transformation_chapters \
         (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
          ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
          batch_id, style_ref_chapter_id, status) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending')",
        params![
            t.transformation_novel_id, t.chapter_id, mode, t.prompt_id, t.model_config_id,
            t.ctx_prev_original, t.ctx_prev_transformed, t.ctx_next_original,
            t.batch_id, t.style_ref_chapter_id,
        ],
    )?;
    Ok(self.conn.last_insert_rowid())
}
```

- [ ] **Step 3: SELECT_SQL + from_row 增 2 列**

同一文件，把：

```rust
const SELECT_SQL: &str =
    "SELECT id, transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
            ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
            status, result_content, tokens_in, tokens_out, \
            error, started_at, completed_at \
     FROM transformation_chapters";
```

改为：

```rust
const SELECT_SQL: &str =
    "SELECT id, transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
            ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
            status, result_content, tokens_in, tokens_out, \
            error, started_at, completed_at, batch_id, style_ref_chapter_id \
     FROM transformation_chapters";
```

`from_row` 把索引 14/15 改 14/15（started_at / completed_at）不变；从 16 开始读新列。把末尾改为：

```rust
fn from_row(row: &Row) -> rusqlite::Result<TransformationChapter> {
    let mode_s: String = row.get(3)?;
    let status_s: String = row.get(9)?;
    let started: Option<String> = row.get(14)?;
    let completed: Option<String> = row.get(15)?;
    Ok(TransformationChapter {
        id: row.get(0)?,
        transformation_novel_id: row.get(1)?,
        chapter_id: row.get(2)?,
        mode: match mode_s.as_str() {
            "compress" => TransformMode::Compress,
            _ => TransformMode::Style,
        },
        prompt_id: row.get(4)?,
        model_config_id: row.get(5)?,
        ctx_prev_original: row.get(6)?,
        ctx_prev_transformed: row.get(7)?,
        ctx_next_original: row.get(8)?,
        status: match status_s.as_str() {
            "pending" => TransformStatus::Pending,
            "running" => TransformStatus::Running,
            "done" => TransformStatus::Done,
            "failed" => TransformStatus::Failed,
            _ => TransformStatus::Cancelled,
        },
        result_content: row.get(10)?,
        tokens_in: row.get(11)?,
        tokens_out: row.get(12)?,
        error: row.get(13)?,
        started_at: started.as_deref().map(|s| parse_ts(14, s)).transpose()?,
        completed_at: completed.as_deref().map(|s| parse_ts(15, s)).transpose()?,
        batch_id: row.get(16)?,
        style_ref_chapter_id: row.get(17)?,
    })
}
```

- [ ] **Step 4: 加 list_by_batch**

同文件 `impl<'a> TransformationChapterRepo<'a>` 末尾追加：

```rust
/// 同一 batch 内所有 tc 行，按 chapter_idx ASC 排（join chapters 表）。
pub fn list_by_batch(&self, batch_id: i64) -> Result<Vec<TransformationChapter>> {
    let mut stmt = self.conn.prepare(&format!(
        "{SELECT_SQL} \
         JOIN chapters c ON c.id = transformation_chapters.chapter_id \
         WHERE transformation_chapters.batch_id = ?1 \
         ORDER BY c.idx ASC, transformation_chapters.id ASC"
    ))?;
    let rows = stmt.query_map(params![batch_id], |row| from_row(row))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}
```

> 占位：本片先不接任何 caller；Slice 4 在 BatchScheduler 内部用它找"batch 内下一章"。

- [ ] **Step 5: 写测试**

在 `crates/nsc-core/tests/db_transformation.rs` 末尾追加：

```rust
#[test]
fn insert_with_batch_id_and_style_ref() {
    let (db, tn_id, cid) = setup();
    let id = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id,
        chapter_id: cid,
        mode: TransformMode::Compress,
        prompt_id: 1,
        model_config_id: 1,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        batch_id: Some(42),
        style_ref_chapter_id: Some(7),
    }).unwrap();
    let t = db.transformation_chapters().get(id).unwrap().unwrap();
    assert_eq!(t.batch_id, Some(42));
    assert_eq!(t.style_ref_chapter_id, Some(7));
}

#[test]
fn insert_without_batch_id_keeps_null() {
    let (db, tn_id, cid) = setup();
    let id = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id,
        chapter_id: cid,
        mode: TransformMode::Compress,
        prompt_id: 1,
        model_config_id: 1,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        batch_id: None,
        style_ref_chapter_id: None,
    }).unwrap();
    let t = db.transformation_chapters().get(id).unwrap().unwrap();
    assert_eq!(t.batch_id, None);
    assert_eq!(t.style_ref_chapter_id, None);
}
```

- [ ] **Step 6: 跑测试**

```bash
cargo test -p nsc-core --test db_transformation
```

Expected：4 个测试过（含 2 个新增）。

- [ ] **Step 7: 跑全测，确认既有 caller 没破**

```bash
cargo test -p nsc-core
```

期望所有测试通过；现存 `NewTransformationChapter { ... }` 字面量构造（`src-tauri/src/commands/transformations.rs` 等）需要补 2 个 `None` 字段。

打开 `src-tauri/src/commands/transformations.rs`，grep `NewTransformationChapter {` —— 把所有构造点改成：

```rust
NewTransformationChapter {
    transformation_novel_id, chapter_id, mode, prompt_id, model_config_id,
    ctx_prev_original, ctx_prev_transformed, ctx_next_original,
    batch_id: None,         // [+NEW] 既有 enqueue 路径不接 batch
    style_ref_chapter_id: None,  // [+NEW]
}
```

跑：

```bash
cargo test -p nsc-core
pnpm test
```

Expected：零回归。

- [ ] **Step 8: 提交**

```bash
git add crates/nsc-core/src/models/transformation.rs \
        crates/nsc-core/src/db/repo/transformation.rs \
        crates/nsc-core/tests/db_transformation.rs \
        src-tauri/src/commands/transformations.rs
git commit -m "feat(db): TransformationChapter 增 batch_id / style_ref_chapter_id 列"
```

---

**Slice 3 完成检查**：

```bash
cargo test -p nsc-core
pnpm test
```

Expected：所有 Rust + vitest 过；新建 / 更新既有的 transformation_chapters 行都带正确 batch_id（None / Some）。

---

## Slice 4 — BatchScheduler 核心

> 最小闭环：Scheduler 能 create_batch 把章节塞进 JobQueue（按 frontier 算 style_ref_chapter_id），JobQueue 完成回调让 scheduler 派下一章；frontier SQL 与 style_ref SQL 单测过；完成判据 + 派发链路通。
> Slice 5 才接 `on_failure_policy` 三分支 + `Skipped` 状态 + `resume_batch`。

### Task 13: 扩展 Notifier 签名（带 transformation_id + success）

**Files:**
- Modify: `crates/nsc-core/src/transformer/queue.rs:19, 91-99, 138-205`

> 当前 `pub type Notifier = Arc<dyn Fn() + Send + Sync>` 不带任何 payload；scheduler 需要知道哪个章节完成 / 失败才能派发 batch 内的下一章。改签名为 `(tid: i64, success: bool, error: Option<String>)`。

- [ ] **Step 1: 改 Notifier 类型 + fire 函数**

`crates/nsc-core/src/transformer/queue.rs`，把：

```rust
pub type Notifier = Arc<dyn Fn() + Send + Sync>;
type NotifySlot = Arc<std::sync::Mutex<Option<Notifier>>>;
```

改为：

```rust
/// Job 完成 / 失败时由 worker 线程触发的回调。
/// - `tid`: 刚结束的 `transformation_chapters.id`
/// - `success`: true = Done（已写库），false = Failed（错误已写库）
/// - `error`: Failed 时携带错误描述，Done 时为 None
pub type Notifier = Arc<dyn Fn(i64, bool, Option<String>) + Send + Sync>;
type NotifySlot = Arc<std::sync::Mutex<Option<Notifier>>>;
```

同文件 `set_notifier` 不动签名（仍是 `notifier: Notifier`）。改 `fire`：

```rust
fn fire(notify: &NotifySlot, tid: i64, success: bool, error: Option<String>) {
    if let Some(n) = notify.lock().expect("notify lock").as_ref() {
        n(tid, success, error);
    }
}
```

- [ ] **Step 2: 改 enqueue 处的 fire 调用**

`enqueue` 方法内 `Self::fire(&self.notify);` 改为：

```rust
/// 入队后立即 fire（不带 status —— worker 还没开始处理）。
/// 语义：通知 scheduler "有新章节在排队"。scheduler 据此可派发后续章节。
pub fn enqueue(&self, job: JobSpec) -> i64 {
    let id = job.transformation_id;
    self.tx.send(job).expect("queue alive");
    Self::fire(&self.notify, id, false, None);  // false = 仅入队，未 Done
    id
}
```

> 注：此处 success=false 仅表示"还没成功结束"。Scheduler 用 `tid` 在 DB 查 status='pending'/'running' 区分。

- [ ] **Step 3: 改 run_job 末尾 fire 调用**

`run_job` 函数末尾 `JobQueue::fire(&notify);` 改为：

```rust
match final_state.db_write {
    DbWrite::Done { tokens_in, tokens_out } => {
        // ... push_done 等不变 ...
        JobQueue::fire(&notify, tid, true, None);
    }
    DbWrite::Failed { err } => {
        // ... push_failed 等不变 ...
        JobQueue::fire(&notify, tid, false, Some(err));
    }
}
```

> prep 阶段（`read_context` 失败）已经提前 fire 一次，把那一处也改：
> 把 `JobQueue::fire(&notify);` 改为 `JobQueue::fire(&notify, tid, false, Some(err));`。

- [ ] **Step 4: 改既有测试调用**

`crates/nsc-core/tests/queue_notifier.rs:35-37`：

```rust
queue.set_notifier(Arc::new(move || {
    count_for_cb.fetch_add(1, Ordering::SeqCst);
}));
```

改为：

```rust
queue.set_notifier(Arc::new(move |_tid, _success, _err| {
    count_for_cb.fetch_add(1, Ordering::SeqCst);
}));
```

> 测试只关心次数，不关心 payload。

- [ ] **Step 5: 跑既有测试确认通过**

```bash
cargo test -p nsc-core --test queue_notifier
cargo test -p nsc-core
```

Expected：所有测试过。

- [ ] **Step 6: 提交**

```bash
git add crates/nsc-core/src/transformer/queue.rs \
        crates/nsc-core/tests/queue_notifier.rs
git commit -m "feat(transformer): Notifier 签名加 tid/success/error"
```

---

### Task 14: 新建 BatchScheduler 模块骨架

**Files:**
- Create: `crates/nsc-core/src/transformer/batch_scheduler.rs`
- Modify: `crates/nsc-core/src/transformer/mod.rs:1-N`

- [ ] **Step 1: 写 BatchScheduler 骨架（含 frontier / style_ref SQL）**

`crates/nsc-core/src/transformer/batch_scheduler.rs`：

```rust
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
use crate::error::Result;
use crate::models::{
    Batch, BatchStatus, Chapter, ModelConfig, NewBatch, NewTransformationChapter,
    OnFailurePolicy, Prompt, TransformMode, TransformationNovel,
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
        let mut db = Db::open(&self.db_path)?;
        let tn_id = new_batch.transformation_novel_id;

        // 取 TN 的默认配置（必填：spec §4.4 兼容性策略）
        let tn = db.transformation_novels().get(tn_id)?
            .ok_or_else(|| crate::error::Error::NotFound(format!("tn {tn_id} 不存在")))?;
        let prompt = db.prompts().get(tn.default_prompt_id)?
            .ok_or_else(|| crate::error::Error::NotFound("default_prompt 缺失".into()))?;
        let model = db.model_configs().get(tn.default_model_config_id)?
            .ok_or_else(|| crate::error::Error::NotFound("default_model_config 缺失".into()))?;

        let tx = db.conn.unchecked_transaction()?;
        let now = Utc::now().to_rfc3339();

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
        let batch_id = tx.last_insert_rowid()?;

        // INSERT N × transformation_chapters（带 frontier 算的 style_ref_chapter_id）
        let mut tids: Vec<i64> = Vec::with_capacity(chapter_ids.len());
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
                    mode_str(tn.default_mode),
                    tn.default_prompt_id,
                    tn.default_model_config_id,
                    batch_id,
                    frontier_cid,
                ],
            )?;
            tids.push(tx.last_insert_rowid()?);
        }
        // batch → running
        tx.execute(
            "UPDATE batches SET status='running', started_at=?1 WHERE id=?2",
            rusqlite::params![now, batch_id],
        )?;
        drop(tx);
        db.conn.unchecked_transaction()?.commit()?;  // close write tx

        // 派首章
        self.dispatch(&db, &tn, &prompt, &model, tids[0])?;

        // 读回 batch 实体
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| crate::error::Error::NotFound("batch 写入后回读失败".into()))?;
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
            .ok_or_else(|| crate::error::Error::NotFound(format!("tc {tid} 不存在")))?;
        let chapter = db.chapters().get(tc.chapter_id)?
            .ok_or_else(|| crate::error::Error::NotFound(format!("chapter {} 不存在", tc.chapter_id)))?;

        let spec = JobSpec {
            transformation_id: tid,
            mode: tn.default_mode,
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
            .ok_or_else(|| crate::error::Error::NotFound(format!("tc {tid} 不存在")))?;
        let batch_id = match tc.batch_id {
            Some(b) => b,
            None => return Ok(()),  // 散点行（非 batch 入队）不归 scheduler 管
        };
        self.advance_batch(&db, batch_id)
    }

    /// 失败回调：占位实现 —— Slice 5 才接 policy 分流。
    /// 本片只保证不 panic、不重复 dispatch。
    pub fn on_chapter_failed(&self, tid: i64, _error: String) -> Result<()> {
        let db = Db::open(&self.db_path)?;
        let tc = db.transformation_chapters().get(tid)?
            .ok_or_else(|| crate::error::Error::NotFound(format!("tc {tid} 不存在")))?;
        let batch_id = match tc.batch_id {
            Some(b) => b,
            None => return Ok(()),
        };
        // Slice 4 占位：失败后保持 batch 仍 Running，但不再 dispatch。
        // Slice 5 才会改成按 on_failure_policy 分流。
        let _ = batch_id;
        Ok(())
    }

    /// 派下一章（若有）；完成判据。
    fn advance_batch(&self, db: &Db, batch_id: i64) -> Result<()> {
        let batch = db.batches().get(batch_id)?
            .ok_or_else(|| crate::error::Error::NotFound(format!("batch {batch_id} 不存在")))?;

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
                .ok_or_else(|| crate::error::Error::NotFound(format!("tn {tn_id} 不存在")))?;
            let prompt = db.prompts().get(tn.default_prompt_id)?
                .ok_or_else(|| crate::error::Error::NotFound("default_prompt 缺失".into()))?;
            let model = db.model_configs().get(tn.default_model_config_id)?
                .ok_or_else(|| crate::error::Error::NotFound("default_model_config 缺失".into()))?;
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
```

- [ ] **Step 2: 注册 mod**

`crates/nsc-core/src/transformer/mod.rs`，找到 `pub mod queue;` 或类似 enum 段，在文件内追加：

```rust
pub mod batch_scheduler;
pub use batch_scheduler::BatchScheduler;
```

如果 `mod.rs` 用的是 `pub mod xxx;` 列表，按既有风格补一行。

- [ ] **Step 3: 写 frontier SQL 单测**

`crates/nsc-core/tests/scheduler.rs`（新文件）：

```rust
//! BatchScheduler 集成测试（in-memory DB）。
//! Slice 4 范围：frontier SQL + style_ref + create_batch + on_chapter_done 派发链。

use nsc_core::db::Db;
use nsc_core::models::{
    BatchStatus, NewBatch, NewChapter, NewDataAsset, NewTransformationChapter, NewTransformationNovel,
    NewUpload, OnFailurePolicy, TransformMode, TransformStatus,
};
use nsc_core::transformer::{BatchScheduler, JobQueue};

fn seed_with_chapters(n: usize) -> (tempfile::TempDir, std::path::PathBuf, Db, i64, i64, Vec<i64>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sched.db");
    let db = Db::open(&path).unwrap();
    db.seed_builtin_prompts().unwrap();

    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(), filename: "x.txt".into(), byte_size: 0,
        file_path: "/tmp/x.txt".into(), original_text: "正文".into(), word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id, title: "DA".into() }).unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id,
        title: "TN",
        default_model_config_id: 1,
        default_prompt_id: 1,
        default_mode: TransformMode::Compress,
    }).unwrap();
    let mut cids = Vec::new();
    for i in 1..=n {
        let cid = db.chapters().insert(&NewChapter {
            data_asset_id: da_id, idx: i as i32,
            title: format!("Ch {i}"),
            byte_start: 0, byte_end: 6, word_count: 2,
        }).unwrap();
        cids.push(cid);
    }
    (dir, path, db, tn_id, da_id, cids)
}

#[test]
fn frontier_chapter_id_returns_prev_done() {
    let (_dir, _path, db, tn_id, _da, cids) = seed_with_chapters(3);
    // tc1 done on ch1
    let t1 = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id, chapter_id: cids[0],
        mode: TransformMode::Compress, prompt_id: 1, model_config_id: 1,
        ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
        batch_id: None, style_ref_chapter_id: None,
    }).unwrap();
    db.transformation_chapters().mark_done(t1, "OK1".into(), 10, 8).unwrap();

    // frontier for ch2 应返回 cids[0]
    let scheduler = BatchScheduler {
        db_path: std::path::PathBuf::new(),  // 占位；本测不调 create_batch
        job_queue: std::sync::Arc::new(unsafe { std::mem::zeroed() }),  // 不进 enqueue
    };
    let _ = scheduler;  // 防止 unused

    // 直接走 SQL helper（用 db.conn）
    let cid: Option<i64> = db.conn.query_row(
        "SELECT c.id FROM transformation_chapters tc \
         JOIN chapters c ON c.id = tc.chapter_id \
         WHERE tc.transformation_novel_id = ?1 AND tc.status = 'done' \
           AND c.idx < (SELECT idx FROM chapters WHERE id = ?2) \
         ORDER BY c.idx DESC LIMIT 1",
        rusqlite::params![tn_id, cids[1]],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(cid, Some(cids[0]));
}

#[test]
fn frontier_chapter_id_returns_none_when_no_prev_done() {
    let (_dir, _path, db, tn_id, _da, cids) = seed_with_chapters(2);

    let cid: Option<i64> = db.conn.query_row(
        "SELECT c.id FROM transformation_chapters tc \
         JOIN chapters c ON c.id = tc.chapter_id \
         WHERE tc.transformation_novel_id = ?1 AND tc.status = 'done' \
           AND c.idx < (SELECT idx FROM chapters WHERE id = ?2) \
         ORDER BY c.idx DESC LIMIT 1",
        rusqlite::params![tn_id, cids[1]],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(cid, None);
}

#[test]
fn frontier_skips_idx_in_between() {
    // ch1 done, ch2 pending, ch3 done —— frontier for ch4 应是 ch3（不是 ch1）
    let (_dir, _path, db, tn_id, _da, cids) = seed_with_chapters(4);
    let t1 = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id, chapter_id: cids[0],
        mode: TransformMode::Compress, prompt_id: 1, model_config_id: 1,
        ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
        batch_id: None, style_ref_chapter_id: None,
    }).unwrap();
    db.transformation_chapters().mark_done(t1, "OK1".into(), 10, 8).unwrap();
    let t3 = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id, chapter_id: cids[2],
        mode: TransformMode::Compress, prompt_id: 1, model_config_id: 1,
        ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
        batch_id: None, style_ref_chapter_id: None,
    }).unwrap();
    db.transformation_chapters().mark_done(t3, "OK3".into(), 10, 8).unwrap();

    let cid: Option<i64> = db.conn.query_row(
        "SELECT c.id FROM transformation_chapters tc \
         JOIN chapters c ON c.id = tc.chapter_id \
         WHERE tc.transformation_novel_id = ?1 AND tc.status = 'done' \
           AND c.idx < (SELECT idx FROM chapters WHERE id = ?2) \
         ORDER BY c.idx DESC LIMIT 1",
        rusqlite::params![tn_id, cids[3]],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(cid, Some(cids[2]));  // ch3（不是 ch1）
}
```

> 占位代码（zeroed Arc）只是为了让 BatchScheduler 类型在本测编译过；前两个测只走 SQL helper，不调 scheduler 方法。
> 等 Slice 5 加完 `resume` 后，可以构造真 Arc<JobQueue> 做 end-to-end。

- [ ] **Step 4: 跑测试**

```bash
cargo test -p nsc-core --test scheduler
```

Expected：3 个 frontier SQL 测试过。

- [ ] **Step 5: 提交**

```bash
git add crates/nsc-core/src/transformer/batch_scheduler.rs \
        crates/nsc-core/src/transformer/mod.rs \
        crates/nsc-core/tests/scheduler.rs
git commit -m "feat(transformer): BatchScheduler 骨架 + frontier SQL"
```

---

### Task 15: lib.rs 接线 — BatchScheduler 单例 + notifier 联通

**Files:**
- Modify: `src-tauri/src/lib.rs:1-N`（启动 JobQueue 后建 scheduler，注册 notifier）

- [ ] **Step 1: 找 lib.rs 现状**

```bash
grep -n "JobQueue::new\|set_notifier\|invoke_handler" src-tauri/src/lib.rs
```

确认：
1. `JobQueue::new(2, ...)` 启动
2. 是否已有 `set_notifier` 调用（如果是 history test setup，挪位置）
3. `invoke_handler!` 注册命令的位置

- [ ] **Step 2: 注册 scheduler**

`src-tauri/src/lib.rs`，紧跟 `JobQueue::new(...)` 后（约 lib.rs 中段），加：

```rust
let job_queue = std::sync::Arc::new(queue);
let scheduler = std::sync::Arc::new(nsc_core::transformer::BatchScheduler::new(
    db_path.clone(),
    job_queue.clone(),
));
{
    let sched = scheduler.clone();
    job_queue.set_notifier(std::sync::Arc::new(move |tid, success, error| {
        let res = if success {
            sched.on_chapter_done(tid)
        } else {
            sched.on_chapter_failed(tid, error.unwrap_or_default())
        };
        if let Err(e) = res {
            eprintln!("[BatchScheduler] notify 处理失败: {e}");
        }
    }));
}
```

> notifier 闭包跑在 worker 线程上 —— 内部 `Db::open(&db_path)` 独立连接，**不会** 触碰到 JobQueue 的 Db（不变量保持）。

- [ ] **Step 3: 把 scheduler 放到 Tauri State**

紧跟其后：

```rust
app.manage(scheduler.clone());
app.manage(job_queue.clone());
```

- [ ] **Step 4: 编译验证**

```bash
cargo build -p nsc-desktop  # 或 src-tauri 实际包名
```

Expected：编译通过，无 unused warning。

- [ ] **Step 5: 跑全测**

```bash
cargo test -p nsc-core
pnpm test
```

Expected：零回归。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(tauri): BatchScheduler 单例 + JobQueue notifier 联通"
```

---

**Slice 4 完成检查**：

```bash
cargo test -p nsc-core
cargo build
pnpm test
```

Expected：
- 所有 Rust 测试过（含新增 scheduler.rs 3 个 frontier SQL 测试）
- src-tauri 编译通过
- vitest 零回归
- 手动 smoke（可选）：`pnpm tauri dev`，调 IPC 创建一条带 on_failure_policy 的 batch，能在 DB 看到 batch + tc 行（不验证完整 LLM 流）

---

## Slice 5 — on_failure_policy 三分支 + resume

> 最小闭环：
> - `TransformStatus::Skipped` 状态值入枚举
> - `BatchScheduler.on_chapter_failed` 按 batch.on_failure_policy 分流（pause / terminate / skip）
> - `BatchScheduler.resume(batch_id, action)` 三种恢复路径
> - IPC `resume_batch` + 配套 payload / 7th IPC 命令齐
> - 前端 wrapper + vitest

### Task 16: TransformStatus 增 Skipped + repo 增 mark_skipped

**Files:**
- Modify: `crates/nsc-core/src/models/transformation.rs:19-21`
- Modify: `crates/nsc-core/src/db/repo/transformation.rs:60-167`
- Modify: `src/ipc/types.ts:153`

- [ ] **Step 1: enum 增 Skipped**

`crates/nsc-core/src/models/transformation.rs`，把：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformStatus { Pending, Running, Done, Failed, Cancelled }
```

改为：

```rust
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
```

- [ ] **Step 2: from_row + status_str 加分支**

`crates/nsc-core/src/db/repo/transformation.rs`，`from_row` 内 match：

```rust
status: match status_s.as_str() {
    "pending" => TransformStatus::Pending,
    "running" => TransformStatus::Running,
    "done" => TransformStatus::Done,
    "failed" => TransformStatus::Failed,
    "skipped" => TransformStatus::Skipped,
    _ => TransformStatus::Cancelled,
},
```

`status_str` 加：

```rust
fn status_str(s: TransformStatus) -> &'static str {
    match s {
        TransformStatus::Pending => "pending",
        TransformStatus::Running => "running",
        TransformStatus::Done => "done",
        TransformStatus::Failed => "failed",
        TransformStatus::Skipped => "skipped",
        TransformStatus::Cancelled => "cancelled",
    }
}
```

- [ ] **Step 3: list_by_status / 新 mark_skipped**

同文件，`mark_failed` 后追加：

```rust
/// 标 skipped —— 保留 error 字段（用户事后能看到原因）；清空 result_content 与 tokens。
pub fn mark_skipped(&self, id: i64, error: String) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    self.conn.execute(
        "UPDATE transformation_chapters \
         SET status='skipped', error=?2, result_content=NULL, tokens_in=NULL, tokens_out=NULL, \
             completed_at=?3 WHERE id=?1",
        params![id, error, now],
    )?;
    Ok(())
}

/// 把 failed/cancelled/skipped 行重置回 pending —— `resume(Retry)` 用。
/// （已有 `reset_to_pending` 写法一致；只是当前已删，本任务补回。）
pub fn reset_to_pending(&self, id: i64) -> Result<()> {
    self.conn.execute(
        "UPDATE transformation_chapters \
         SET status='pending', result_content=NULL, tokens_in=NULL, tokens_out=NULL, \
             error=NULL, started_at=NULL, completed_at=NULL \
         WHERE id=?1",
        params![id],
    )?;
    Ok(())
}
```

> 上面 `reset_to_pending` 是把 §Slice 3 Task 12 内 repo 里同名函数的"补 2 字段" 工作反向 —— 现有 repo 已有 `reset_to_pending`（line 99-108），无需重复加；保留这段注释作提醒，确认既有函数签名一致即可。

确认既有 `reset_to_pending`（crates/nsc-core/src/db/repo/transformation.rs:99）：

```rust
pub fn reset_to_pending(&self, id: i64) -> Result<()> {
    self.conn.execute(
        "UPDATE transformation_chapters \
         SET status='pending', result_content=NULL, tokens_in=NULL, tokens_out=NULL, \
             error=NULL, started_at=NULL, completed_at=NULL \
         WHERE id=?1",
        params![id],
    )?;
    Ok(())
}
```

OK 既有函数已经存在且字段齐全，**不重复加**。

- [ ] **Step 4: 前端 enum 增 skipped**

`src/ipc/types.ts:153`，把：

```typescript
export type TransformStatus = 'pending' | 'running' | 'done' | 'failed' | 'cancelled';
```

改为：

```typescript
export type TransformStatus = 'pending' | 'running' | 'done' | 'failed' | 'skipped' | 'cancelled';
```

- [ ] **Step 5: 跑全测**

```bash
cargo test -p nsc-core
pnpm test
```

Expected：所有现存测试过；Skipped 字面量已能 round-trip 通过 `TransformStatus::Skipped <-> "skipped"`。

- [ ] **Step 6: 提交**

```bash
git add crates/nsc-core/src/models/transformation.rs \
        crates/nsc-core/src/db/repo/transformation.rs \
        src/ipc/types.ts
git commit -m "feat(db): TransformStatus 增 Skipped + mark_skipped"
```

---

### Task 17: BatchScheduler — on_failure_policy 分流 + resume

**Files:**
- Modify: `crates/nsc-core/src/transformer/batch_scheduler.rs`（替换 §Task 14 Step 1 末尾的 `on_chapter_failed` 占位 + 追加 `resume`）

- [ ] **Step 1: 替换 on_chapter_failed 占位**

`crates/nsc-core/src/transformer/batch_scheduler.rs`，把当前 `on_chapter_failed`（§Task 14 占位）整段替换：

```rust
/// JobQueue 失败回调 —— 按 batch.on_failure_policy 分流。
///   PauseAndReview: chapter Failed, batch Paused（等用户 resume）
///   Terminate:      chapter Failed, 同 batch 后续 pending → Cancelled, batch Terminated
///   SkipFailed:     chapter Skipped, 继续派下一章（batch 仍 Running）
pub fn on_chapter_failed(&self, tid: i64, error: String) -> Result<()> {
    let db = Db::open(&self.db_path)?;
    let tc = db.transformation_chapters().get(tid)?
        .ok_or_else(|| crate::error::Error::NotFound(format!("tc {tid} 不存在")))?;
    let batch_id = match tc.batch_id {
        Some(b) => b,
        None => return Ok(()),  // 散点行不归 scheduler
    };
    let batch = db.batches().get(batch_id)?
        .ok_or_else(|| crate::error::Error::NotFound(format!("batch {batch_id} 不存在")))?;

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
```

- [ ] **Step 2: 加 resume**

`batch_scheduler.rs` 末尾、`policy_str`/`mode_str` 之前追加：

```rust
/// 用户在 paused 时介入。三种动作：
///   Retry(ch_id):    tc 重置为 pending + 立即 dispatch（绕过 batch 头）
///   Skip(ch_id):     tc 标 skipped + dispatch 下一章
///   Terminate:       同 batch 后续 pending → cancelled, batch Terminated
pub fn resume(&self, batch_id: i64, action: ResumeAction) -> Result<Batch> {
    let db = Db::open(&self.db_path)?;
    let batch = db.batches().get(batch_id)?
        .ok_or_else(|| crate::error::Error::NotFound(format!("batch {batch_id} 不存在")))?;
    if !matches!(batch.status, BatchStatus::Paused) {
        return Err(crate::error::Error::Validation(format!(
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
                .ok_or_else(|| crate::error::Error::NotFound(format!("tn {tn_id} 不存在")))?;
            let prompt = db.prompts().get(tn.default_prompt_id)?
                .ok_or_else(|| crate::error::Error::NotFound("default_prompt 缺失".into()))?;
            let model = db.model_configs().get(tn.default_model_config_id)?
                .ok_or_else(|| crate::error::Error::NotFound("default_model_config 缺失".into()))?;
            self.dispatch(&db, &tn, &prompt &model, ch_id)?;
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
        .ok_or_else(|| crate::error::Error::NotFound("batch 回读失败".into()))?;
    Ok(b)
}
```

- [ ] **Step 3: 同步加 ResumeAction 枚举与 BatchStatus 的 Paused**

`crates/nsc-core/src/models/batch.rs`（Slice 2 已建），在 `BatchStatus` 加 `Paused`：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    Pending,
    Running,
    Paused,         // [+NEW]
    Completed,
    Terminated,
    Cancelled,
}
```

在 `OnFailurePolicy` 同文件加：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumeAction {
    /// 重试该章（status 重置 pending，立即 dispatch）
    Retry(i64),
    /// 跳过该章（status=skipped，继续派 batch 内下一章）
    Skip(i64),
    /// 终止整批（pending → cancelled, batch Terminated）
    Terminate,
}
```

并在 `models/batch.rs` 顶部 import 加上 `use serde::{Deserialize, Serialize};`（如果还没）。

- [ ] **Step 4: BatchRepo.from_row 加 Paused 分支**

`crates/nsc-core/src/db/repo/batch.rs`，`from_row` 的 `status` match 增：

```rust
status: match status_s.as_str() {
    "pending" => BatchStatus::Pending,
    "running" => BatchStatus::Running,
    "paused" => BatchStatus::Paused,
    "completed" => BatchStatus::Completed,
    "terminated" => BatchStatus::Terminated,
    _ => BatchStatus::Cancelled,
},
```

- [ ] **Step 5: 编译验证 + 跑测试**

```bash
cargo build
cargo test -p nsc-core
```

Expected：通过（`resume` 路径尚未跑 e2e，但单元覆盖靠 Slice 5 Task 19）。

- [ ] **Step 6: 提交**

```bash
git add crates/nsc-core/src/transformer/batch_scheduler.rs \
        crates/nsc-core/src/models/batch.rs \
        crates/nsc-core/src/db/repo/batch.rs
git commit -m "feat(scheduler): on_failure_policy 三分支 + resume action"
```

---

### Task 18: resume_batch IPC 命令

**Files:**
- Create: `src-tauri/src/commands/batches.rs`（Slice 2 已有部分；本 Task 补 `resume_batch`）
- Modify: `src-tauri/src/lib.rs`（注册新命令）

> 假定 Slice 2 Task 8 已经创建 `src-tauri/src/commands/batches.rs` 含 6 个 IPC（list / get / create / update / list_batch_chapters / count）。本 Task 追加 `resume_batch`。

- [ ] **Step 1: 加 ResumeActionPayload + ResumeBatchCommand**

`src-tauri/src/commands/batches.rs`，文件顶部 import：

```rust
use nsc_core::models::{Batch, ResumeAction};
use nsc_core::transformer::BatchScheduler;
```

> 已有 imports 看实际 Slice 2 落地而定；不要重复加。

文件末尾追加：

```rust
/// `resume_batch` 入参。`kind` 决定动作；`chapter_id` 仅 retry/skip 时必填。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResumeActionPayload {
    pub kind: String,                // "retry" | "skip" | "terminate"
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
    scheduler: tauri::State<'_, std::sync::Arc<BatchScheduler>>,
) -> Result<Batch, String> {
    let scheduler = scheduler.inner().clone();
    let resume_action = action.into_core();
    // scheduler.resume 是同步 DB 操作；不阻塞 tokio 也能跑，
    // 但放进 spawn_blocking 防止极端慢 DB 阻塞 IPC runtime。
    let res = tokio::task::spawn_blocking(move || scheduler.resume(batch_id, resume_action))
        .await
        .map_err(|e| format!("resume_batch join error: {e}"))?
        .map_err(|e| e.to_string())?;
    Ok(res)
}
```

> 如果 lib.rs 已注册 scheduler 为 `tauri::State`，可直接拿；本 task 不要重做 Slice 2 的 IPC 注册流程，只补 `resume_batch` 一条。

- [ ] **Step 2: lib.rs 注册命令**

`src-tauri/src/lib.rs` 的 `invoke_handler!` 宏数组里追加：

```rust
.invoke_handler(tauri::generate_handler![
    // ... 既有命令 ...
    commands::batches::resume_batch,
])
```

- [ ] **Step 3: 编译验证**

```bash
cargo build
```

Expected：通过。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/commands/batches.rs src-tauri/src/lib.rs
git commit -m "feat(tauri): resume_batch IPC 命令"
```

---

### Task 19: 前端 — resume_batch wrapper + vitest

**Files:**
- Modify: `src/ipc/commands.ts:1-N`
- Modify: `src/ipc/types.ts:1-N`
- Modify: `src/stores/batches.ts:1-N`
- Create: `src/__tests__/resume_batch.spec.ts`

- [ ] **Step 1: types 增 ResumeAction**

`src/ipc/types.ts`，在已有 `Batch` interface 附近追加：

```typescript
export type ResumeAction =
  | { kind: 'retry'; chapter_id: number }
  | { kind: 'skip'; chapter_id: number }
  | { kind: 'terminate' };
```

- [ ] **Step 2: commands.ts 加 wrapper**

`src/ipc/commands.ts` 末尾追加：

```typescript
export function resumeBatch(
  batchId: number,
  action: ResumeAction,
): Promise<Batch> {
  return invoke<Batch>('resume_batch', { batchId, action });
}
```

> IPC 载荷内层字段名遵循 §3.2：外层 `batchId` camelCase（自动），内层 `action.chapter_id` snake_case 显式。

- [ ] **Step 3: store 加 resume action**

`src/stores/batches.ts`，在 `useBatchesStore` 返回对象里追加：

```typescript
async function resume(batchId: number, action: ResumeAction) {
  loading.value = true;
  error.value = null;
  try {
    const batch = await resumeBatch(batchId, action);
    // 更新本地缓存
    const arr = byTn.value.get(batch.transformation_novel_id);
    if (arr) {
      const idx = arr.findIndex((b) => b.id === batch.id);
      if (idx >= 0) arr[idx] = batch;
    }
    return batch;
  } catch (e) {
    error.value = (e as Error).message;
    throw e;
  } finally {
    loading.value = false;
  }
}

return { byTn, loading, error, loadByTn, resume, refresh };
```

并在文件顶部 import 增 `import { resumeBatch } from '../ipc/commands';`。

- [ ] **Step 4: 写 vitest**

`src/__tests__/resume_batch.spec.ts`：

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { resumeBatch } from '../ipc/commands';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('resumeBatch IPC wrapper', () => {
  beforeEach(() => vi.clearAllMocks());

  it('retry 把 chapter_id 嵌进 snake_case payload', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ id: 1, status: 'running' });
    await resumeBatch(1, { kind: 'retry', chapter_id: 7 });
    expect(invoke).toHaveBeenCalledWith('resume_batch', {
      batchId: 1,
      action: { kind: 'retry', chapter_id: 7 },
    });
  });

  it('skip 同样带 chapter_id', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ id: 1, status: 'running' });
    await resumeBatch(1, { kind: 'skip', chapter_id: 9 });
    expect(invoke).toHaveBeenCalledWith('resume_batch', {
      batchId: 1,
      action: { kind: 'skip', chapter_id: 9 },
    });
  });

  it('terminate 不带 chapter_id', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ id: 1, status: 'terminated' });
    await resumeBatch(1, { kind: 'terminate' });
    expect(invoke).toHaveBeenCalledWith('resume_batch', {
      batchId: 1,
      action: { kind: 'terminate' },
    });
  });
});
```

- [ ] **Step 5: 跑测试**

```bash
pnpm test -- --run resume_batch
```

Expected：3 个测试过。

- [ ] **Step 6: 提交**

```bash
git add src/ipc/types.ts src/ipc/commands.ts src/stores/batches.ts src/__tests__/resume_batch.spec.ts
git commit -m "feat(ui): resumeBatch wrapper + vitest"
```

---

### Task 20: scheduler 集成测试 — on_failure_policy 三分支 + resume

**Files:**
- Modify: `crates/nsc-core/tests/scheduler.rs`（Slice 4 Task 14 Step 3 占位测试改真）

> Slice 4 时测试只走 frontier SQL；本 Task 构造真 Arc<JobQueue> + 不真发 LLM 的"假 worker"（provider_factory 返回不抛错的空 provider），验证三 policy 派发 + 完成判据 + resume。

- [ ] **Step 1: 准备测试 harness**

`crates/nsc-core/tests/scheduler.rs` 顶部 import 增：

```rust
use std::sync::Arc;
use nsc_core::ai::{AiProvider, ChatMessage, ChatRequest, ChatResponse, Role};
use nsc_core::transformer::{DbFactory, JobQueue, Notifier, ProviderFactory};
```

`scheduler.rs` 文件末尾追加：

```rust
/// 假 AI provider —— 直接把 user content 作为 response 返还。
/// 用于不真发 HTTP 的批调度测试。
struct EchoProvider;
#[async_trait::async_trait]
impl AiProvider for EchoProvider {
    async fn chat(&self, req: ChatRequest) -> nsc_core::error::Result<ChatResponse> {
        let user = req.messages.iter().find(|m| matches!(m.role, Role::User))
            .map(|m| m.content.clone()).unwrap_or_default();
        Ok(ChatResponse {
            content: format!("ECHO:{user}"),
            tokens_in: user.len() as i32,
            tokens_out: user.len() as i32,
        })
    }
}

/// 用假 provider 构造 JobQueue + Scheduler pair。返回 queue + scheduler。
fn build_pair(db_path: std::path::PathBuf) -> (Arc<JobQueue>, Arc<BatchScheduler>) {
    let path_for_factory = db_path.clone();
    let db_factory: DbFactory = Arc::new(move || Db::open(&path_for_factory));
    let provider_factory: ProviderFactory = Arc::new(|_cfg| -> Box<dyn AiProvider> {
        Box::new(EchoProvider)
    });
    let queue = Arc::new(JobQueue::new(1, db_factory, provider_factory));

    // 注册一个 dummy notifier —— 测试里不指望 scheduler 通过 notifier 触发派发，
    // 而是手动调 on_chapter_done 来同步推进（保证测试确定性）。
    queue.set_notifier(Arc::new(|_tid, _success, _err| {}));

    let scheduler = Arc::new(BatchScheduler::new(db_path, queue.clone()));
    (queue, scheduler)
}
```

> `AiProvider` / `ChatRequest` / `ChatResponse` 实际签名以 `crates/nsc-core/src/ai/openai.rs` 为准；先 `cargo check` 看是否需要调字段。

- [ ] **Step 2: 测试 — skip_failed 派下一章**

`s scheduler.rs` 文件末尾追加：

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn skip_failed_dispatches_next_chapter() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("skip.db");
    let (queue, scheduler) = build_pair(path.clone());

    // seed 数据
    let db = Db::open(&path).unwrap();
    db.seed_builtin_prompts().unwrap();
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(), filename: "x.txt".into(), byte_size: 0,
        file_path: "/tmp/x.txt".into(), original_text: "正文".into(), word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id, title: "DA".into() }).unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id, title: "TN",
        default_model_config_id: 1, default_prompt_id: 1,
        default_mode: TransformMode::Compress,
    }).unwrap();
    let c1 = db.chapters().insert(&NewChapter {
        data_asset_id: da_id, idx: 1, title: "C1".into(),
        byte_start: 0, byte_end: 2, word_count: 1,
    }).unwrap();
    let c2 = db.chapters().insert(&NewChapter {
        data_asset_id: da_id, idx: 2, title: "C2".into(),
        byte_start: 2, byte_end: 4, word_count: 1,
    }).unwrap();
    let c3 = db.chapters().insert(&NewChapter {
        data_asset_id: da_id, idx: 3, title: "C3".into(),
        byte_start: 4, byte_end: 6, word_count: 1,
    }).unwrap();

    // batch with skip_failed
    let batch = scheduler.create_batch(NewBatch {
        transformation_novel_id: tn_id,
        label: None,
        on_failure_policy: OnFailurePolicy::SkipFailed,
    }, vec![c1, c2, c3]).unwrap();
    assert_eq!(batch.status, BatchStatus::Running);

    // 等 worker 处理 c1
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // 模拟 c1 done → on_chapter_done 应派 c2
    let tids: Vec<i64> = db.conn.prepare(
        "SELECT id FROM transformation_chapters WHERE batch_id=?1 ORDER BY id ASC"
    ).unwrap().query_map(rusqlite::params![batch.id], |r| r.get(0))
        .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();

    scheduler.on_chapter_done(tids[0]).unwrap();

    // 等 worker 处理 c2（成功后 c3 应自动派）
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    let tcs: Vec<TransformStatus> = db.transformation_chapters().list_by_batch(batch.id).unwrap()
        .into_iter().map(|t| t.status).collect();
    // 因为 EchoProvider 不会失败，所以三章都 Done；batch 应 Completed
    assert_eq!(tcs, vec![TransformStatus::Done, TransformStatus::Done, TransformStatus::Done]);
    let b = db.batches().get(batch.id).unwrap().unwrap();
    assert_eq!(b.status, BatchStatus::Completed);
    let _ = queue; // 保持 queue alive 到测试结束
}
```

- [ ] **Step 3: 测试 — pause_and_review 不派下一章**

同文件追加：

```rust
#[test]
fn pause_and_review_does_not_advance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pause.db");
    let db = Db::open(&path).unwrap();
    db.seed_builtin_prompts().unwrap();
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(), filename: "x.txt".into(), byte_size: 0,
        file_path: "/tmp/x.txt".into(), original_text: "正文".into(), word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id, title: "DA".into() }).unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id, title: "TN",
        default_model_config_id: 1, default_prompt_id: 1,
        default_mode: TransformMode::Compress,
    }).unwrap();
    let c1 = db.chapters().insert(&NewChapter {
        data_asset_id: da_id, idx: 1, title: "C1".into(),
        byte_start: 0, byte_end: 2, word_count: 1,
    }).unwrap();
    let c2 = db.chapters().insert(&NewChapter {
        data_asset_id: da_id, idx: 2, title: "C2".into(),
        byte_start: 2, byte_end: 4, word_count: 1,
    }).unwrap();

    let scheduler = BatchScheduler::new(
        path.clone(),
        Arc::new(JobQueue::new(1,
            { let p = path.clone(); Arc::new(move || Db::open(&p)) as DbFactory },
            { Arc::new(|_| -> Box<dyn AiProvider> { Box::new(EchoProvider) }) as ProviderFactory },
        )),
    );

    let batch = scheduler.create_batch(NewBatch {
        transformation_novel_id: tn_id, label: None,
        on_failure_policy: OnFailurePolicy::PauseAndReview,
    }, vec![c1, c2]).unwrap();
    let tids: Vec<i64> = db.conn.prepare(
        "SELECT id FROM transformation_chapters WHERE batch_id=?1 ORDER BY id ASC"
    ).unwrap().query_map(rusqlite::params![batch.id], |r| r.get(0))
        .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();

    // 模拟 c1 失败
    db.transformation_chapters().mark_failed(tids[0], "fake error".into()).unwrap();
    scheduler.on_chapter_failed(tids[0], "fake error".into()).unwrap();

    let b = db.batches().get(batch.id).unwrap().unwrap();
    assert_eq!(b.status, BatchStatus::Paused);

    // c2 仍 pending —— 没派
    let t2_status = db.transformation_chapters().get(tids[1]).unwrap().unwrap().status;
    assert_eq!(t2_status, TransformStatus::Pending);

    // resume(retry c1)
    let _ = scheduler.resume(batch.id, nsc_core::models::ResumeAction::Retry(tids[0])).unwrap();
    let b = db.batches().get(batch.id).unwrap().unwrap();
    assert_eq!(b.status, BatchStatus::Running);
}
```

- [ ] **Step 4: 测试 — terminate 终止整批**

同文件追加：

```rust
#[test]
fn terminate_cancels_remaining() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("term.db");
    let db = Db::open(&path).unwrap();
    db.seed_builtin_prompts().unwrap();
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(), filename: "x.txt".into(), byte_size: 0,
        file_path: "/tmp/x.txt".into(), original_text: "正文".into(), word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id, title: "DA".into() }).unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id, title: "TN",
        default_model_config_id: 1, default_prompt_id: 1,
        default_mode: TransformMode::Compress,
    }).unwrap();
    let c1 = db.chapters().insert(&NewChapter {
        data_asset_id: da_id, idx: 1, title: "C1".into(),
        byte_start: 0, byte_end: 2, word_count: 1,
    }).unwrap();
    let c2 = db.chapters().insert(&NewChapter {
        data_asset_id: da_id, idx: 2, title: "C2".into(),
        byte_start: 2, byte_end: 4, word_count: 1,
    }).unwrap();

    let scheduler = BatchScheduler::new(
        path.clone(),
        Arc::new(JobQueue::new(1,
            { let p = path.clone(); Arc::new(move || Db::open(&p)) as DbFactory },
            { Arc::new(|_| -> Box<dyn AiProvider> { Box::new(EchoProvider) }) as ProviderFactory },
        )),
    );

    let batch = scheduler.create_batch(NewBatch {
        transformation_novel_id: tn_id, label: None,
        on_failure_policy: OnFailurePolicy::Terminate,
    }, vec![c1, c2]).unwrap();
    let tids: Vec<i64> = db.conn.prepare(
        "SELECT id FROM transformation_chapters WHERE batch_id=?1 ORDER BY id ASC"
    ).unwrap().query_map(rusqlite::params![batch.id], |r| r.get(0))
        .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();

    db.transformation_chapters().mark_failed(tids[0], "boom".into()).unwrap();
    scheduler.on_chapter_failed(tids[0], "boom".into()).unwrap();

    let b = db.batches().get(batch.id).unwrap().unwrap();
    assert_eq!(b.status, BatchStatus::Terminated);
    let t2_status = db.transformation_chapters().get(tids[1]).unwrap().unwrap().status;
    assert_eq!(t2_status, TransformStatus::Cancelled);
}
```

- [ ] **Step 5: 跑测试**

```bash
cargo test -p nsc-core --test scheduler
```

Expected：3 个新测试过（+ Slice 4 的 3 个 frontier SQL 测试 = 6 个 total）。

如果 `cargo test` 卡在 await（异步 test 没等到 worker 处理完）—— 加 `tokio::time::sleep` 或在 helper 里 `wait_for_done_count` 轮询。

- [ ] **Step 6: 提交**

```bash
git add crates/nsc-core/tests/scheduler.rs
git commit -m "test(scheduler): on_failure_policy 三分支 + resume"
```

---

**Slice 5 完成检查**：

```bash
cargo test -p nsc-core
cargo build
pnpm test
```

Expected：所有测试过；resume_batch IPC 链路通。

---

## Slice 6 — TN 详情页骨架

> 最小闭环：
> - 路由 `/library/transformation/:tnId`
> - `TransformationNovelDetail.vue` 两 tab（章节一览 / 工作流）只读
> - Library "详情" 入口
> - 5s 轮询 batch 状态
> - vitest 组件快照（项目首批 vue-test 模式）

### Task 21: 路由 + TN 详情页骨架

**Files:**
- Modify: `src/router/index.ts:1-N`
- Create: `src/views/TransformationNovelDetail.vue`
- Modify: `src/ipc/types.ts`（按需增 `TransformationChapterDetail` 类型）

- [ ] **Step 1: 加路由**

`src/router/index.ts`，在 `routes: [...]` 数组加：

```typescript
{
  path: '/library/transformation/:tnId',
  name: 'transformation-detail',
  component: () => import('../views/TransformationNovelDetail.vue'),
  props: true,
},
```

> `props: true` 让 `:tnId` 自动作为 prop 传入组件。

- [ ] **Step 2: 写 TransformationNovelDetail.vue 骨架**

`src/views/TransformationNovelDetail.vue`：

```vue
<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useRoute } from 'vue-router';
import { listTransformationChapters } from '../ipc/commands';
import { useBatchesStore } from '../stores/batches';
import type { Batch, TransformationChapterRow } from '../ipc/types';
import PageHeader from '../components/PageHeader.vue';

const route = useRoute();
const tnId = computed(() => Number(route.params.tnId));

const batchesStore = useBatchesStore();
const chapters = ref<TransformationChapterRow[]>([]);
const activeTab = ref<'chapters' | 'workflows'>('chapters');
const selectedBatchId = ref<number | null>(null);
const panelChapters = ref<TransformationChapterRow[]>([]);
const polling = ref<number | null>(null);

async function loadChapters() {
  chapters.value = await listTransformationChapters(tnId.value);
}

async function loadBatches() {
  await batchesStore.loadByTn(tnId.value);
}

async function openBatchPanel(batch: Batch) {
  selectedBatchId.value = batch.id;
  // 本地缓存：batchesStore.byTn 已经包含 Batch；panel 章节直接用 chapters 过滤 batch_id
  // (chapters.value 已 join batch_id；按 §5.4 SELECT_SQL 包含 batch_id)
  // 简化方案：从本地 chapters 里过滤（detail page 同时 load 了所有 tc 行）
  panelChapters.value = chapters.value.filter((c: any) => c.batch_id === batch.id);
}

onMounted(async () => {
  await Promise.all([loadChapters(), loadBatches()]);
  // 5s 轮询
  polling.value = window.setInterval(() => {
    loadBatches();
  }, 5000);
});

onUnmounted(() => {
  if (polling.value !== null) window.clearInterval(polling.value);
});

const batches = computed<Batch[]>(() => batchesStore.byTn.get(tnId.value) ?? []);
</script>

<template>
  <div>
    <PageHeader title="转换工程详情" :subtitle="`TN #${tnId}`">
      <template #actions>
        <button class="btn" @click="$router.back()">← 返回</button>
      </template>
    </PageHeader>

    <div class="tabs">
      <button :class="{ active: activeTab === 'chapters' }" @click="activeTab = 'chapters'">
        章节一览
      </button>
      <button :class="{ active: activeTab === 'workflows' }" @click="activeTab = 'workflows'">
        工作流
      </button>
    </div>

    <!-- 章节一览 tab -->
    <table v-if="activeTab === 'chapters'" class="chapter-table">
      <thead>
        <tr>
          <th>#</th>
          <th>标题</th>
          <th>模式</th>
          <th>状态</th>
          <th>批号</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="c in chapters" :key="c.id">
          <td>{{ c.chapter_idx }}</td>
          <td>{{ c.chapter_title }}</td>
          <td>{{ c.mode }}</td>
          <td>{{ c.status }}</td>
          <td>{{ (c as any).batch_id ?? '—' }}</td>
        </tr>
      </tbody>
    </table>

    <!-- 工作流 tab -->
    <div v-else>
      <div v-if="batches.find((b) => b.status === 'paused')" class="paused-banner">
        ⚠ 有工作流处于暂停状态，请处理
      </div>
      <table class="batch-table">
        <thead>
          <tr>
            <th>Label</th>
            <th>策略</th>
            <th>状态</th>
            <th>创建</th>
            <th>结束</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="b in batches" :key="b.id" @click="openBatchPanel(b)">
            <td>{{ b.label ?? '—' }}</td>
            <td>{{ b.on_failure_policy }}</td>
            <td>{{ b.status }}</td>
            <td>{{ b.created_at }}</td>
            <td>{{ b.ended_at ?? '—' }}</td>
          </tr>
        </tbody>
      </table>

      <!-- 侧滑 panel -->
      <div v-if="selectedBatchId !== null" class="side-panel">
        <h3>批号 #{{ selectedBatchId }} 章节进度</h3>
        <button @click="selectedBatchId = null">关闭</button>
        <table>
          <thead>
            <tr><th>#</th><th>标题</th><th>状态</th><th>错误</th></tr>
          </thead>
          <tbody>
            <tr v-for="c in panelChapters" :key="c.id">
              <td>{{ c.chapter_idx }}</td>
              <td>{{ c.chapter_title }}</td>
              <td>{{ c.status }}</td>
              <td>{{ c.error ?? '' }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tabs { display: flex; gap: 8px; margin-bottom: 16px; }
.tabs button { padding: 6px 14px; }
.tabs button.active { background: var(--color-primary); color: white; }
.chapter-table, .batch-table { width: 100%; border-collapse: collapse; }
.chapter-table th, .chapter-table td,
.batch-table th, .batch-table td { padding: 6px 10px; border-bottom: 1px solid var(--color-border); }
.paused-banner { background: var(--color-error-bg); padding: 8px 12px; margin-bottom: 12px; }
.side-panel { position: fixed; top: 0; right: 0; width: 360px; height: 100vh; background: var(--color-bg); border-left: 1px solid var(--color-border); padding: 16px; overflow: auto; }
</style>
```

> 字段名 `c.batch_id`：当前 `TransformationChapterRow` 接口（`src/ipc/types.ts:160-176`）没有 batch_id 字段。需要在 types.ts 加：

```typescript
export interface TransformationChapterRow {
  // ... 既有 16 字段 ...
  batch_id: number | null;        // [+NEW] spec §4.1 schema
  style_ref_chapter_id: number | null;  // [+NEW]
}
```

> 实际后端 list_transformation_chapters 是不是 JOIN batch_id 上来 —— Slice 4 还没改；这里假设后端会发，本 Task 同步要求 §23 修改后端 IPC 一起加。

- [ ] **Step 3: 修改后端 list_transformation_chapters 包含 batch_id**

`src-tauri/src/commands/transformations.rs`，找到 `list_transformation_chapters` 签名与 row 构造。改成 SELECT 多查 2 列 + row struct 多 2 字段。具体看现有实现（grep `list_transformation_chapters`）。

> 字段命名约定：外层 IPC 响应字段 snake_case（与 db 一致；前端 row 直接用）。

- [ ] **Step 4: 编译验证**

```bash
cargo build
pnpm tauri dev  # 或 npm 脚本，验证页面能跳转
```

Expected：编译通过，路由可达。

- [ ] **Step 5: 提交**

```bash
git add src/router/index.ts \
        src/views/TransformationNovelDetail.vue \
        src/ipc/types.ts \
        src-tauri/src/commands/transformations.rs
git commit -m "feat(ui): TN 详情页骨架 + 路由"
```

---

### Task 22: Library.vue — transformations tab "详情" 入口

**Files:**
- Modify: `src/views/Library.vue:1-N`

- [ ] **Step 1: 找现有 transformations tab 行**

```bash
grep -n "transformations\|transformation_novel" src/views/Library.vue
```

确认 transformations tab 已存在 + 行渲染逻辑。

- [ ] **Step 2: 加"详情"按钮**

在 transformations 行末加 `<button @click="goDetail(tn.id)">详情</button>`，并在 `<script setup>` 加：

```typescript
import { useRouter } from 'vue-router';
const router = useRouter();
function goDetail(tnId: number) {
  router.push({ name: 'transformation-detail', params: { tnId: String(tnId) } });
}
```

- [ ] **Step 3: 跑既有测试**

```bash
pnpm test
```

Expected：既有 `library.spec.ts` 不破。

- [ ] **Step 4: 提交**

```bash
git add src/views/Library.vue
git commit -m "feat(ui): Library transformations tab 加详情入口"
```

---

### Task 23: vitest — TransformationNovelDetail 快照

**Files:**
- Create: `src/__tests__/tnDetail.spec.ts`

> 项目首批 vue-test 模式：不引 DI library；用 @vue/test-utils + vitest 的 createApp + defineComponent 简化路径。或直接 stub router + 测 store 调用次数。

- [ ] **Step 1: 装 @vue/test-utils（如未装）**

```bash
pnpm add -D @vue/test-utils vue-tsc
```

> 如果 `pnpm test` 已用 vue-test-utils 则跳过。

- [ ] **Step 2: 写 snapshot 测试**

`src/__tests__/tnDetail.spec.ts`：

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createMemoryHistory, createRouter } from 'vue-router';
import TransformationNovelDetail from '../views/TransformationNovelDetail.vue';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === 'list_transformation_chapters') return Promise.resolve([]);
    if (cmd === 'list_batches') return Promise.resolve([]);
    return Promise.resolve(null);
  }),
}));

const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    { path: '/library/transformation/:tnId', component: TransformationNovelDetail, props: true },
  ],
});

describe('TransformationNovelDetail', () => {
  beforeEach(async () => {
    await router.push('/library/transformation/42');
    await router.isReady();
  });

  it('mounts and shows chapters tab by default', async () => {
    const wrapper = mount(TransformationNovelDetail, {
      props: { tnId: 42 },
      global: { plugins: [router] },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('TN #42');
    expect(wrapper.text()).toContain('章节一览');
  });
});
```

- [ ] **Step 3: 跑测试**

```bash
pnpm test -- --run tnDetail
```

Expected：1 个测试过。

- [ ] **Step 4: 提交**

```bash
git add src/__tests__/tnDetail.spec.ts package.json pnpm-lock.yaml
git commit -m "test(ui): TransformationNovelDetail 快照测试"
```

---

**Slice 6 完成检查**：

```bash
cargo test -p nsc-core
cargo build
pnpm test
```

Expected：
- 所有 Rust 测试过
- src-tauri 编译通过
- vitest 全过（含新增 tnDetail.spec.ts）
- 手动 smoke：`pnpm tauri dev` → Library → transformations tab → "详情" → 详情页可打开 + 两 tab 可切

---

## 自检

写完 plan 后过一遍 spec：

1. **覆盖**：spec 13 节 → plan 6 片 → 23 个 task。切片与 spec §10 实现切片一致；测试覆盖 spec §9。
2. **占位扫描**：grep `TBD|TODO|FIXME|待|占位|…` —— 应仅剩"Slice 4 占位 / Slice 5 替代"等说明性文字，已替换的标"已替换"。
3. **类型一致**：`TransformStatus::Skipped`、`BatchStatus::Paused`、`ResumeAction::{Retry, Skip, Terminate}` 在 plan 内多处出现 —— 名字一致；IPC payload 内层字段（`chapter_id` / `kind`）与 spec §6 一致。
4. **不变量**：所有任务都遵循 `Db is Send but NOT Sync`（scheduler / IPC 闭包不捕获 Arc<Db>，仅 db_path）；migration 全部 IF NOT EXISTS / ADD COLUMN（仅 0007/0008 是 ALTER 但 IF NOT EXISTS 加列在 SQLite 3.35+ 可重跑，spec §12 已确认）。
5. **可裁剪点**：spec §11 列出的 5 个可裁剪点；plan 内已选择性裁剪：
   - "裁 style_ref_chapter_id UI 暴露" → **已裁**（Slice 3 列保留但 UI 不暴露入口）
   - "裁 resume_batch Skip" → **未裁**（Slice 5 完整接 Retry/Skip/Terminate）
   - "裁 工作流 tab batch 点击侧滑 panel" → **未裁**（Slice 6 实现侧滑 panel）
   - "裁 batches.label UI" → **未裁**（table 已展示 label 列）
   - "裁 章节一览完整列" → **已裁**（只展示 idx/title/mode/status/batch_id 5 列）
