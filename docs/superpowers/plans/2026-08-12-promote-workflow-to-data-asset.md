# 工作流转正数据资产 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 workflow Stopped 后的产物手动派生为独立数据资产；工作流层与数据资产层继续是平级独立实体，promoted da 可独立使用、独立删除。

**Architecture:** 破坏性扩展 `data_assets` 加 `kind` / `source_workflow_id` / `source_data_asset_id` / `note`（migration 0021），保留所有 da 平级，UI 通过 `kind` 列区分"源 / 派生"。`promote_workflow` 走单事务：前置校验 → 写入 promoted da → 写入 N 个 chapter。允许重复转正（append-only）；删源 da / 删 workflow 走 `SET NULL` 保留 promoted da 独立性。

**Tech Stack:** Rust (rusqlite, nsc-core repo + scheduler, Tauri 2 IPC) / Vue 3 + Pinia + Vite / SQLite migration。

**Spec:** `docs/superpowers/specs/2026-08-12-promote-workflow-to-data-asset-design.md`

---

## 文件结构

**新增**:
- `migrations/0021_data_asset_kind.sql` — migration 加 4 列 + 2 索引
- `crates/nsc-core/src/models/data_asset_kind.rs` — `DataAssetKind` enum
- `crates/nsc-core/src/db/repo/promotion.rs` — `create_promoted_from_workflow` 核心事务
- `crates/nsc-core/tests/promote_workflow.rs` — 集成测试
- `src/components/PromoteWorkflowDialog.vue` — 转正弹窗

**修改**:
- `crates/nsc-core/src/db/migrate.rs` — 注册 migration 0021
- `crates/nsc-core/src/models/mod.rs` — 导出新模块
- `crates/nsc-core/src/models/data_asset.rs` — `DataAsset` 加 4 字段
- `crates/nsc-core/src/models/batch.rs` — `WorkflowSummary` 加 `promoted_count`
- `crates/nsc-core/src/db/repo/mod.rs` — 导出新模块
- `crates/nsc-core/src/db/repo/data_assets.rs` — `DataAssetWithUpload` 加字段；`list_with_upload` 改 JOIN；新增 `count_promoted_by_workflow` / `list_promoted_by_workflow` / `list_by_upload`
- `crates/nsc-core/src/db/repo/workflows.rs` — `list_by_tn` / `get` 返回带 `promoted_count`（新 query）
- `src-tauri/src/commands/data_assets.rs` — 新增 4 IPC
- `src-tauri/src/commands/workflows.rs` — `list_by_tn` / `get` 返回带 `promoted_count`
- `src-tauri/src/lib.rs` — 注册新 IPC
- `src/ipc/types.ts` — `DataAsset` / `DataAssetWithUpload` / `WorkflowSummary` 加字段；新增 `PromoteWorkflowInput`
- `src/ipc/commands.ts` — 新增 4 IPC wrapper
- `src/stores/dataAsset.ts` — 加 getter / action
- `src/stores/workflows.ts` — 加 `promoted_count` 字段透传
- `src/views/Library.vue` — 类型列 + 派生数列 + 行点击跳转
- `src/views/TransformationNovelDetail.vue` — 转正按钮 + 弹窗 + 已转正 tag
- `src/views/Upload.vue` — 派生数列
- `src/views/parse.vue` — promoted da 只读模式 + source_kind 列

**删除**: 无

## 风险/约束

- migration 0021 是破坏性 ALTER，测试阶段允许清库重建，不写回滚脚本
- 自引用 FK `data_assets.source_data_asset_id`：转正时必须先确认源 da 已存在（必然存在），再 INSERT 新 promoted da
- `Db::open` 与 `Db::connect` 已拆分（之前 commit）：repo 层用 `Db::connect`，migration 用 `Db::open`
- worker / IPC / 前端不持 `Arc<Db>` 共享，按路径 reopen
- 测试覆盖 §9.1 所有路径（前置校验 / 填充规则 / ON DELETE 矩阵）

---
## Task 1: Migration 0021 — data_assets 加 kind + 溯源字段

**Files:**
- Create: `migrations/0021_data_asset_kind.sql`
- Modify: `crates/nsc-core/src/db/migrate.rs:6`

- [ ] **Step 1: 创建 migration 文件**

```sql
-- migrations/0021_data_asset_kind.sql
ALTER TABLE data_assets ADD COLUMN kind TEXT NOT NULL DEFAULT 'source';
ALTER TABLE data_assets ADD COLUMN source_workflow_id INTEGER REFERENCES batches(id) ON DELETE SET NULL;
ALTER TABLE data_assets ADD COLUMN source_data_asset_id INTEGER REFERENCES data_assets(id) ON DELETE SET NULL;
ALTER TABLE data_assets ADD COLUMN note TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_data_assets_kind ON data_assets(kind);
CREATE INDEX IF NOT EXISTS idx_data_assets_source_workflow ON data_assets(source_workflow_id);
```

- [ ] **Step 2: 注册 migration**

修改 `crates/nsc-core/src/db/migrate.rs` 在 `SCHEMAS` 数组末尾追加：

```rust
("0021_data_asset_kind", include_str!("../../../../migrations/0021_data_asset_kind.sql")),
```

- [ ] **Step 3: 跑 migration 验证**

```bash
cd D:/Git/novel-style-converter
rm -f nsc.db
cargo test -p nsc-core --lib db::pool::tests::opens_in_memory_and_seeds_schema -- --nocapture
```

期望：`test result: ok. 1 passed`。

- [ ] **Step 4: 验证现有 da 自动有 kind='source'**

写一个简单测试 `crates/nsc-core/tests/migration_0021.rs`：

```rust
use nsc_core::db::Db;
use nsc_core::models::{DataAssetKind, NewDataAsset};

#[test]
fn migration_0021_existing_assets_default_to_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let db = Db::open(&path).unwrap();
    let id = db.data_assets().insert(&NewDataAsset {
        upload_id: 1,
        title: "t".into(),
        source_filename: "f".into(),
    }).unwrap();
    let reloaded = db.data_assets().get(id).unwrap().unwrap();
    assert_eq!(reloaded.kind, DataAssetKind::Source);
    assert!(reloaded.source_workflow_id.is_none());
    assert!(reloaded.source_data_asset_id.is_none());
}
```

跑：`cargo test -p nsc-core --lib migration_0021 -- --nocapture`
期望：`1 passed`。

- [ ] **Step 5: Commit**

```bash
git add migrations/0021_data_asset_kind.sql crates/nsc-core/src/db/migrate.rs crates/nsc-core/tests/migration_0021.rs
git commit -m "feat(db): migration 0021 — data_assets 加 kind + 溯源字段"
```

---

## Task 2: DataAssetKind enum + DataAsset 加字段

**Files:**
- Create: `crates/nsc-core/src/models/data_asset_kind.rs`
- Modify: `crates/nsc-core/src/models/mod.rs`
- Modify: `crates/nsc-core/src/models/data_asset.rs`

- [ ] **Step 1: 创建 enum 文件**

```rust
// crates/nsc-core/src/models/data_asset_kind.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataAssetKind {
    Source,
    Promoted,
}

impl DataAssetKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Promoted => "promoted",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "source" => Some(Self::Source),
            "promoted" => Some(Self::Promoted),
            _ => None,
        }
    }
}
```

- [ ] **Step 2: 注册模块**

修改 `crates/nsc-core/src/models/mod.rs`，在文件顶部追加：

```rust
pub mod data_asset_kind;
pub use data_asset_kind::DataAssetKind;
```

- [ ] **Step 3: 扩 DataAsset 结构体**

修改 `crates/nsc-core/src/models/data_asset.rs`：

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::DataAssetKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAsset {
    pub id: i64,
    pub upload_id: i64,
    pub title: String,
    pub parsed_at: DateTime<Utc>,
    pub source_filename: String,
    pub kind: DataAssetKind,
    pub source_workflow_id: Option<i64>,
    pub source_data_asset_id: Option<i64>,
    pub note: String,
}

#[derive(Debug, Clone)]
pub struct NewDataAsset {
    pub upload_id: i64,
    pub title: String,
    pub source_filename: String,
}
```

- [ ] **Step 4: 跑现有测试看哪里 break**

```bash
cd D:/Git/novel-style-converter
cargo build -p nsc-core 2>&1 | head -50
```

期望：编译报错指出 DataAsset 字段缺失（data_assets repo 的 from_row 函数未更新）。进入 Task 3 修。

- [ ] **Step 5: Commit**

```bash
git add crates/nsc-core/src/models/data_asset_kind.rs crates/nsc-core/src/models/mod.rs crates/nsc-core/src/models/data_asset.rs
git commit -m "feat(models): DataAssetKind enum + DataAsset 加 kind/source_workflow_id/source_data_asset_id/note"
```

---
## Task 3: data_assets repo — DataAsset 加字段读取

**Files:**
- Modify: `crates/nsc-core/src/db/repo/data_assets.rs`

- [ ] **Step 1: 修改 `from_row` 解析新字段**

修改 `crates/nsc-core/src/db/repo/data_assets.rs` 的 `from_row`：

```rust
fn from_row(row: &Row) -> rusqlite::Result<DataAsset> {
    let parsed_at_s: String = row.get(3)?;
    let parsed_at = DateTime::parse_from_rfc3339(&parsed_at_s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
    let kind_s: String = row.get(5)?;
    let kind = DataAssetKind::parse(&kind_s).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(format!("unknown kind: {kind_s}")))
    })?;
    let source_workflow_id: Option<i64> = row.get(6)?;
    let source_data_asset_id: Option<i64> = row.get(7)?;
    let note: String = row.get(8)?;
    Ok(DataAsset {
        id: row.get(0)?,
        upload_id: row.get(1)?,
        title: row.get(2)?,
        parsed_at,
        source_filename: row.get(4)?,
        kind,
        source_workflow_id,
        source_data_asset_id,
        note,
    })
}
```

并更新所有 `SELECT id, upload_id, title, parsed_at, source_filename FROM data_assets` → 末尾追加 `, kind, source_workflow_id, source_data_asset_id, note`，涉及：

- `get`
- `list`
- `find_by_upload`
- `list_with_upload`（在 Task 6 改）

- [ ] **Step 2: 跑测试验证**

```bash
cd D:/Git/novel-style-converter
cargo test -p nsc-core --lib db::repo::data_assets -- --nocapture
```

期望：通过；任何依赖 DataAsset 字段的旧测试要么通过要么被 Task 6 / 7 修复。

- [ ] **Step 3: Commit**

```bash
git add crates/nsc-core/src/db/repo/data_assets.rs
git commit -m "refactor(repo): data_assets 读取新字段"
```

---

## Task 4: `create_promoted_from_workflow` 单事务核心（TDD）

**Files:**
- Create: `crates/nsc-core/src/db/repo/promotion.rs`
- Modify: `crates/nsc-core/src/db/repo/mod.rs`

- [ ] **Step 1: 写失败测试**

创建 `crates/nsc-core/src/db/repo/promotion.rs` 暂时只放测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::models::{BatchStatus, NewBatch, NewTransformationChapter,
        NewTransformationNovel, NewUpload, OnFailurePolicy, TransformMode};

    fn fresh_db() -> Db {
        let dir = tempfile::tempdir().unwrap();
        Db::open(&dir.path().join("test.db")).unwrap()
    }

    fn seed_full_chain(db: &Db) -> (i64, i64, i64, Vec<i64>) {
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "x".into(), filename: "f.txt".into(),
            byte_size: 10, file_path: "/tmp/f.txt".into(),
        }).unwrap();
        let da_id = db.data_assets().insert(&crate::models::NewDataAsset {
            upload_id, title: "源".into(), source_filename: "f.txt".into(),
        }).unwrap();
        let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
            data_asset_id: da_id, title: "tn".into(),
        }).unwrap();
        let batch_id = db.batches().insert(&NewBatch {
            transformation_novel_id: tn_id, label: Some("w1".into()),
            on_failure_policy: OnFailurePolicy::PauseAndReview,
        }).unwrap();
        let mut tc_ids = vec![];
        for i in 0..3 {
            let chapter_id = db.chapters().insert(&crate::models::NewChapter {
                data_asset_id: da_id, idx: i + 1,
                title: format!("c{}", i + 1),
                body: format!("原文章{}很长很长", i + 1),
                word_count: 5,
                ..Default::default()
            }).unwrap();
            let tc_id = db.transformation_chapters().insert(&NewTransformationChapter {
                transformation_novel_id: tn_id, chapter_id,
                mode: TransformMode::Compress, prompt_id: 1, model_config_id: 1,
                ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
                batch_id: Some(batch_id), style_ref_chapter_id: None,
            }).unwrap();
            tc_ids.push(tc_id);
        }
        (upload_id, da_id, batch_id, tc_ids)
    }

    #[test]
    fn promote_creates_da_with_promoted_kind_and_chapters() {
        let db = fresh_db();
        let (_up, da_id, batch_id, tc_ids) = seed_full_chain(&db);
        db.transformation_chapters().mark_done(tc_ids[0], "转换后文本A".into(), 10, 20).unwrap();
        db.batches().set_status(batch_id, BatchStatus::Stopped).unwrap();

        let new_da_id = db.promotion().create_promoted_from_workflow(batch_id, "派生测试".into()).unwrap();

        let new_da = db.data_assets().get(new_da_id).unwrap().unwrap();
        assert_eq!(new_da.kind, DataAssetKind::Promoted);
        assert_eq!(new_da.source_workflow_id, Some(batch_id));
        assert_eq!(new_da.source_data_asset_id, Some(da_id));
        assert_eq!(new_da.title, "派生测试");

        let chapters = db.chapters().list_by_data_asset(new_da_id).unwrap();
        assert_eq!(chapters.len(), 3);
        assert_eq!(chapters[0].body, "转换后文本A");
        assert_eq!(chapters[0].source_kind, "transformed");
        assert!(chapters[1].body.starts_with("原文章2"));
        assert_eq!(chapters[1].source_kind, "original");
        assert!(chapters[2].body.starts_with("原文章3"));
        assert_eq!(chapters[2].source_kind, "original");
    }

    #[test]
    fn promote_rejects_non_stopped_batch() {
        let db = fresh_db();
        let (_u, _d, batch_id, _) = seed_full_chain(&db);
        db.batches().set_status(batch_id, BatchStatus::Stopped).unwrap(); // 先 stopped
        // 改成 running 再测
        db.batches().set_status(batch_id, BatchStatus::Running).unwrap();
        let err = db.promotion().create_promoted_from_workflow(batch_id, "t".into()).unwrap_err();
        assert!(format!("{err:?}").contains("Stopped"));
    }

    #[test]
    fn promote_rejects_done_tc_with_null_content() {
        let db = fresh_db();
        let (_u, _d, batch_id, tc_ids) = seed_full_chain(&db);
        db.conn.execute(
            "UPDATE transformation_chapters SET status='done' WHERE id=?1",
            rusqlite::params![tc_ids[0]],
        ).unwrap();
        db.batches().set_status(batch_id, BatchStatus::Stopped).unwrap();
        let err = db.promotion().create_promoted_from_workflow(batch_id, "t".into()).unwrap_err();
        assert!(format!("{err:?}").contains("数据损坏") || format!("{err:?}").contains("Validation"));
    }

    #[test]
    fn promote_allows_repeat_appends_new_da() {
        let db = fresh_db();
        let (_u, _d, batch_id, tc_ids) = seed_full_chain(&db);
        db.transformation_chapters().mark_done(tc_ids[0], "A".into(), 1, 1).unwrap();
        db.batches().set_status(batch_id, BatchStatus::Stopped).unwrap();
        let id1 = db.promotion().create_promoted_from_workflow(batch_id, "v1".into()).unwrap();
        let id2 = db.promotion().create_promoted_from_workflow(batch_id, "v2".into()).unwrap();
        assert_ne!(id1, id2);
        assert_eq!(db.promotion().count_by_workflow(batch_id).unwrap(), 2);
    }

    #[test]
    fn delete_source_da_sets_null_on_promoted() {
        let db = fresh_db();
        let (_u, da_id, batch_id, tc_ids) = seed_full_chain(&db);
        db.transformation_chapters().mark_done(tc_ids[0], "A".into(), 1, 1).unwrap();
        db.batches().set_status(batch_id, BatchStatus::Stopped).unwrap();
        let promoted_id = db.promotion().create_promoted_from_workflow(batch_id, "p".into()).unwrap();
        db.data_assets().delete(da_id).unwrap();
        let promoted_after = db.data_assets().get(promoted_id).unwrap().unwrap();
        assert!(promoted_after.source_data_asset_id.is_none());
        assert_eq!(promoted_after.kind, DataAssetKind::Promoted);
    }
}
```

- [ ] **Step 2: 跑测试看失败**

```bash
cd D:/Git/novel-style-converter
cargo test -p nsc-core --lib db::repo::promotion -- --nocapture 2>&1 | tail -20
```

期望：编译失败，提示 `db.promotion()` / `create_promoted_from_workflow` 不存在。

- [ ] **Step 3: 实现 promotion 模块 + 接入 repo trait**

在 `crates/nsc-core/src/db/mod.rs` 找到 `Db` 的 repo accessor 模式，添加：

```rust
impl Db {
    pub fn promotion(&self) -> crate::db::repo::promotion::PromotionRepo<'_> {
        crate::db::repo::promotion::PromotionRepo { conn: &self.conn }
    }
}
```

实现 `crates/nsc-core/src/db/repo/promotion.rs`：

```rust
use chrono::Utc;
use rusqlite::params;
use crate::error::{Error, Result};
use crate::models::{DataAsset, DataAssetKind};

pub struct PromotionRepo<'a> { pub(crate) conn: &'a rusqlite::Connection }

impl<'a> PromotionRepo<'a> {
    pub fn create_promoted_from_workflow(&self, batch_id: i64, title: String) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let now = Utc::now().to_rfc3339();

        // 1. batch 存在且 stopped
        let batch_status: String = tx.query_row(
            "SELECT status FROM batches WHERE id=?1",
            params![batch_id], |r| r.get(0),
        ).map_err(|_| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        if batch_status != "stopped" {
            return Err(Error::Validation(format!("workflow 必须 Stopped 才能转正(当前 {batch_status})")));
        }

        // 2. 读 source_data_asset_id 和 upload_id
        let source_da_id: i64 = tx.query_row(
            "SELECT tn.data_asset_id FROM transformation_novels tn
             JOIN batches b ON b.transformation_novel_id = tn.id WHERE b.id = ?1",
            params![batch_id], |r| r.get(0),
        )?;
        let upload_id: i64 = tx.query_row(
            "SELECT upload_id FROM data_assets WHERE id=?1",
            params![source_da_id], |r| r.get(0),
        )?;

        // 3. 读所有 tc + chapter + wrc
        let mut stmt = tx.prepare(
            "SELECT tc.id, tc.chapter_id, tc.status,
                    c.idx, c.title, c.body, c.word_count,
                    wrc.content
             FROM transformation_chapters tc
             JOIN chapters c ON c.id = tc.chapter_id
             LEFT JOIN workflow_results wr ON wr.batch_id = tc.batch_id
             LEFT JOIN workflow_result_chapters wrc
                 ON wrc.workflow_result_id = wr.id AND wrc.chapter_id = tc.chapter_id
             WHERE tc.batch_id = ?1 ORDER BY c.idx ASC",
        )?;
        let rows: Vec<(i64, i64, String, i32, String, String, i32, Option<String>)> = stmt
            .query_map(params![batch_id], |r| Ok((
                r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?,
                r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?,
            )))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if rows.is_empty() {
            return Err(Error::Validation("workflow 无章节".into()));
        }

        // 4. 前置校验
        for (tc_id, _cid, tc_status, _idx, _t, _b, _wc, wrc_content) in &rows {
            match tc_status.as_str() {
                "done" => {
                    if wrc_content.is_none() {
                        return Err(Error::Validation(format!("数据损坏:tc {tc_id} done 但 wrc.content IS NULL")));
                    }
                }
                "failed" | "skipped" => {}
                other => {
                    return Err(Error::Validation(format!("workflow 含未完成任务(tc {tc_id} status={other})")));
                }
            }
        }

        // 5. INSERT promoted da
        tx.execute(
            "INSERT INTO data_assets
                (upload_id, title, parsed_at, source_filename, kind, source_workflow_id, source_data_asset_id, note)
             VALUES (?1, ?2, ?3, '', 'promoted', ?4, ?5, '')",
            params![upload_id, title, now, batch_id, source_da_id],
        )?;
        let new_da_id = tx.last_insert_rowid();

        // 6. INSERT N 个 chapter
        for (_tc_id, chapter_id, tc_status, idx, chapter_title, chapter_body, word_count, wrc_content) in &rows {
            let (body, source_kind) = if tc_status == "done" {
                (wrc_content.as_ref().unwrap().clone(), "transformed".to_string())
            } else {
                (chapter_body.clone(), "original".to_string())
            };
            tx.execute(
                "INSERT INTO chapters
                    (data_asset_id, idx, title, body, word_count, source_kind, source_chapter_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![new_da_id, idx, chapter_title, body, word_count, source_kind, chapter_id],
            )?;
        }

        tx.commit()?;
        Ok(new_da_id)
    }

    pub fn count_by_workflow(&self, batch_id: i64) -> Result<i64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM data_assets WHERE source_workflow_id = ?1",
            params![batch_id], |r| r.get(0),
        )?;
        Ok(n)
    }

    pub fn list_by_workflow(&self, batch_id: i64) -> Result<Vec<DataAsset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, upload_id, title, parsed_at, source_filename, kind, source_workflow_id, source_data_asset_id, note
             FROM data_assets WHERE source_workflow_id = ?1 ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![batch_id], |row| {
            let parsed_at_s: String = row.get(3)?;
            let parsed_at = chrono::DateTime::parse_from_rfc3339(&parsed_at_s)
                .map(|d| d.with_timezone(&chrono::Utc))
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
            let kind_s: String = row.get(5)?;
            let kind = DataAssetKind::parse(&kind_s).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(format!("unknown kind: {kind_s}")))
            })?;
            Ok(DataAsset {
                id: row.get(0)?, upload_id: row.get(1)?, title: row.get(2)?, parsed_at,
                source_filename: row.get(4)?, kind,
                source_workflow_id: row.get(6)?, source_data_asset_id: row.get(7)?,
                note: row.get(8)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn list_by_upload(&self, upload_id: i64) -> Result<Vec<DataAsset>> {
        // 类似 list_by_workflow,WHERE upload_id = ?1
        // 完整实现参考 list_by_workflow,改 SQL WHERE 子句
        todo!("参考 list_by_workflow 实现,WHERE upload_id = ?1")
    }
}
```

注册模块到 `crates/nsc-core/src/db/repo/mod.rs`：

```rust
pub mod promotion;
```

- [ ] **Step 4: 跑测试验证**

```bash
cd D:/Git/novel-style-converter
cargo test -p nsc-core --lib db::repo::promotion -- --nocapture 2>&1 | tail -30
```

期望：`5 passed`。

- [ ] **Step 5: Commit**

```bash
git add crates/nsc-core/src/db/repo/promotion.rs crates/nsc-core/src/db/repo/mod.rs crates/nsc-core/src/db/mod.rs
git commit -m "feat(repo): create_promoted_from_workflow 单事务核心 + 计数/查询辅助"
```

---
## Task 5: chapters 表加 source_kind + source_chapter_id 字段

**Files:**
- Modify: `migrations/0021_data_asset_kind.sql`
- Modify: `crates/nsc-core/src/models/chapter.rs`
- Modify: `crates/nsc-core/src/db/repo/chapters.rs`

> 步骤顺序调整：先在 migration 0021 末尾追加列，跑一次 migration；再修 Chapter 模型和 chapter repo。最后跑全测试。

- [ ] **Step 1: migration 追加列**

在 `migrations/0021_data_asset_kind.sql` 末尾追加：

```sql
ALTER TABLE chapters ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'original';
ALTER TABLE chapters ADD COLUMN source_chapter_id INTEGER REFERENCES chapters(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_chapters_source_kind ON chapters(data_asset_id, source_kind);
```

- [ ] **Step 2: Chapter 模型加字段**

修改 `crates/nsc-core/src/models/chapter.rs`：

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: i64,
    pub data_asset_id: i64,
    pub idx: i32,
    pub title: String,
    pub body: String,
    pub word_count: i32,
    pub source_kind: String,
    pub source_chapter_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewChapter {
    pub data_asset_id: i64,
    pub idx: i32,
    pub title: String,
    pub body: String,
    pub word_count: i32,
    pub source_kind: String,
    pub source_chapter_id: Option<i64>,
}

impl Default for NewChapter {
    fn default() -> Self {
        Self {
            data_asset_id: 0, idx: 0, title: String::new(),
            body: String::new(), word_count: 0,
            source_kind: "original".into(), source_chapter_id: None,
        }
    }
}
```

- [ ] **Step 3: chapter repo 更新 from_row + insert + insert_many + replace**

修改 `crates/nsc-core/src/db/repo/chapters.rs` 的 `chapter_from_row`：

```rust
fn chapter_from_row(row: &Row<'_>) -> rusqlite::Result<Chapter> {
    Ok(Chapter {
        id: row.get(0)?, data_asset_id: row.get(1)?, idx: row.get(2)?,
        title: row.get(3)?, body: row.get(4)?, word_count: row.get(5)?,
        source_chapter_id: row.get(6)?, source_kind: row.get(7)?,
    })
}
```

所有 `SELECT id, data_asset_id, idx, title, body, word_count FROM chapters` 末尾追加 `, source_chapter_id, source_kind`（6 列 → 8 列）。

`insert` 和 `insert_many` SQL 末尾加 `, source_kind, source_chapter_id` 两列 + 对应 params。

`replace_all_for_data_asset` 同样处理。

- [ ] **Step 4: 跑全测试**

```bash
cd D:/Git/novel-style-converter
cargo test -p nsc-core --lib 2>&1 | tail -20
```

期望：旧 22 + Task 1/2/3/4/5 新增 6 = 28 测试全 pass。如有 NewChapter 没填 source_kind/source_chapter_id 的 caller 报错，逐个补 Default。

- [ ] **Step 5: Commit**

```bash
git add migrations/0021_data_asset_kind.sql crates/nsc-core/src/models/chapter.rs crates/nsc-core/src/db/repo/chapters.rs
git commit -m "feat(db): chapters.source_kind + source_chapter_id"
```

---

## Task 6: DataAssetWithUpload 加字段 + list_with_upload 改 JOIN

**Files:**
- Modify: `crates/nsc-core/src/db/repo/data_assets.rs`

- [ ] **Step 1: 改 DataAssetWithUpload 结构**

```rust
pub struct DataAssetWithUpload {
    pub id: i64,
    pub upload_id: i64,
    pub title: String,
    pub parsed_at: DateTime<Utc>,
    pub filename: String,
    pub byte_size: i64,
    pub word_count: i64,
    pub tn_count: i64,
    pub kind: DataAssetKind,
    pub source_workflow_id: Option<i64>,
    pub source_data_asset_id: Option<i64>,
    pub promoted_count: i64,
}
```

- [ ] **Step 2: 改 list_with_upload 的 SQL**

```rust
pub fn list_with_upload(&self) -> Result<Vec<DataAssetWithUpload>> {
    let mut stmt = self.conn.prepare(
        "SELECT da.id, da.upload_id, da.title, da.parsed_at,
                COALESCE(u.filename, da.source_filename) AS filename,
                COALESCE(u.byte_size, 0) AS byte_size,
                COALESCE((SELECT SUM(c.word_count) FROM chapters c WHERE c.data_asset_id = da.id), 0) AS word_count,
                COALESCE(tn.cnt, 0) AS tn_count,
                da.kind, da.source_workflow_id, da.source_data_asset_id,
                COALESCE(da_derived.cnt, 0) AS promoted_count
         FROM data_assets da
         LEFT JOIN uploads u ON u.id = da.upload_id
         LEFT JOIN (SELECT data_asset_id, COUNT(*) AS cnt
                    FROM transformation_novels GROUP BY data_asset_id) tn
                ON tn.data_asset_id = da.id
         LEFT JOIN (SELECT source_data_asset_id, COUNT(*) AS cnt
                    FROM data_assets WHERE source_data_asset_id IS NOT NULL
                    GROUP BY source_data_asset_id) da_derived
                ON da_derived.source_data_asset_id = da.id
         GROUP BY da.id
         ORDER BY da.id DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        let parsed_at_s: String = row.get(3)?;
        let parsed_at = DateTime::parse_from_rfc3339(&parsed_at_s)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
        let kind_s: String = row.get(8)?;
        let kind = DataAssetKind::parse(&kind_s).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(format!("unknown kind: {kind_s}")))
        })?;
        Ok(DataAssetWithUpload {
            id: row.get(0)?, upload_id: row.get(1)?, title: row.get(2)?, parsed_at,
            filename: row.get(4)?, byte_size: row.get(5)?,
            word_count: row.get(6)?, tn_count: row.get(7)?,
            kind,
            source_workflow_id: row.get(9)?, source_data_asset_id: row.get(10)?,
            promoted_count: row.get(11)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}
```

- [ ] **Step 3: 跑测试**

```bash
cd D:/Git/novel-style-converter
cargo test -p nsc-core --lib db::repo::data_assets -- --nocapture 2>&1 | tail -15
```

期望：通过。

- [ ] **Step 4: Commit**

```bash
git add crates/nsc-core/src/db/repo/data_assets.rs
git commit -m "feat(repo): DataAssetWithUpload 加 kind/source_*/promoted_count"
```

---

## Task 7: WorkflowSummary 加 promoted_count + 修改 list/get

**Files:**
- Modify: `crates/nsc-core/src/models/batch.rs`
- Modify: `crates/nsc-core/src/db/repo/workflows.rs`

- [ ] **Step 1: WorkflowSummary 加字段**

修改 `crates/nsc-core/src/models/batch.rs`：

```rust
pub struct WorkflowSummary {
    // ... 现有字段 ...
    pub promoted_count: i64,  // 新增
}
```

按实际字段顺序插入。

- [ ] **Step 2: 修改 workflows.rs 查询**

`list_by_tn` 和 `get` 的 SQL JOIN：

```sql
SELECT b.*, COALESCE(da_promoted.cnt, 0) AS promoted_count
FROM batches b
LEFT JOIN (SELECT source_workflow_id, COUNT(*) AS cnt
           FROM data_assets WHERE source_workflow_id IS NOT NULL
           GROUP BY source_workflow_id) da_promoted
    ON da_promoted.source_workflow_id = b.id
WHERE b.transformation_novel_id = ?1
ORDER BY b.id DESC
```

在 row_to_summary 里读 `promoted_count` 列。

- [ ] **Step 3: 跑测试**

```bash
cd D:/Git/novel-style-converter
cargo test -p nsc-core --lib db::repo::workflows -- --nocapture 2>&1 | tail -15
```

期望：通过。

- [ ] **Step 4: Commit**

```bash
git add crates/nsc-core/src/models/batch.rs crates/nsc-core/src/db/repo/workflows.rs
git commit -m "feat(repo): WorkflowSummary 加 promoted_count + list/get 返回派生数"
```

---
## Task 8: Tauri IPC 层加 4 个新命令 + 修改 list_data_assets/list_workflows/get_workflow

**Files:**
- Modify: `src-tauri/src/commands/data_assets.rs`
- Modify: `src-tauri/src/commands/workflows.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: data_assets.rs 加 promote 等命令**

在 `src-tauri/src/commands/data_assets.rs` 末尾追加：

```rust
#[tauri::command]
pub async fn promote_workflow(
    db_path: State<'_, Arc<std::path::PathBuf>>,
    batch_id: i64,
    title: String,
) -> Result<nsc_core::models::DataAsset, String> {
    let db = Db::connect(&**db_path).map_err(|e| e.to_string())?;
    let new_id = db.promotion().create_promoted_from_workflow(batch_id, title).map_err(|e| e.to_string())?;
    db.data_assets().get(new_id).map_err(|e| e.to_string())?.ok_or_else(|| "新 da 找不到".to_string())
}

#[tauri::command]
pub fn count_promoted_data_assets_by_workflow(
    db_path: State<'_, Arc<std::path::PathBuf>>,
    batch_id: i64,
) -> Result<i64, String> {
    let db = Db::connect(&**db_path).map_err(|e| e.to_string())?;
    db.promotion().count_by_workflow(batch_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_promoted_data_assets_for_workflow(
    db_path: State<'_, Arc<std::path::PathBuf>>,
    batch_id: i64,
) -> Result<Vec<nsc_core::models::DataAsset>, String> {
    let db = Db::connect(&**db_path).map_err(|e| e.to_string())?;
    db.promotion().list_by_workflow(batch_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_data_assets_by_upload(
    db_path: State<'_, Arc<std::path::PathBuf>>,
    upload_id: i64,
) -> Result<Vec<nsc_core::models::DataAsset>, String> {
    let db = Db::connect(&**db_path).map_err(|e| e.to_string())?;
    db.promotion().list_by_upload(upload_id).map_err(|e| e.to_string())
}
```

注意：`State<'_, Arc<std::path::PathBuf>>` 需要 `src-tauri/src/lib.rs` 在 setup 阶段 `.manage(Arc::new(path.clone()))`。

- [ ] **Step 2: lib.rs 注册新命令 + manage db_path State**

修改 `src-tauri/src/lib.rs` 的 `run()` 函数：

```rust
// 在 .setup(|app| { ... }) 里
let db_path_arc: Arc<std::path::PathBuf> = Arc::new(path.clone());
app.manage(db_path_arc);

// 在 invoke_handler 里加新命令
.invoke_handler(tauri::generate_handler![
    // ... 现有命令 ...
    commands::data_assets::promote_workflow,
    commands::data_assets::count_promoted_data_assets_by_workflow,
    commands::data_assets::list_promoted_data_assets_for_workflow,
    commands::data_assets::list_data_assets_by_upload,
])
```

- [ ] **Step 3: 修改现有 list_data_assets 返回类型**

`list_data_assets` 返回 `DataAssetWithUpload[]`，Task 6 已经改了结构，IPC 层自动跟着变。检查 serde rename 是否一致。

- [ ] **Step 4: 修改 workflows.rs 的 list/get**

`list_workflows` 和 `get_workflow` 返回 `WorkflowSummary`，Task 7 已经改了结构，IPC 层自动跟着变。

- [ ] **Step 5: 编译验证**

```bash
cd D:/Git/novel-style-converter
cargo check --workspace 2>&1 | tail -30
```

期望：通过。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/data_assets.rs src-tauri/src/commands/workflows.rs src-tauri/src/lib.rs
git commit -m "feat(ipc): promote_workflow 等 4 个新命令,list_data_assets/list_workflows 返回 promoted_count"
```

---

## Task 9: 前端 types + commands wrappers

**Files:**
- Modify: `src/ipc/types.ts`
- Modify: `src/ipc/commands.ts`

- [ ] **Step 1: 扩展 types**

在 `src/ipc/types.ts`：

```typescript
export type DataAssetKind = 'source' | 'promoted';

export interface DataAsset {
  id: number;
  upload_id: number;
  title: string;
  parsed_at: string;
  source_filename: string;
  kind: DataAssetKind;
  source_workflow_id: number | null;
  source_data_asset_id: number | null;
  note: string;
}

export interface DataAssetWithUpload extends DataAsset {
  filename: string;
  byte_size: number;
  word_count: number;
  tn_count: number;
  promoted_count: number;
}

export interface WorkflowSummary {
  // ... 现有字段 ...
  promoted_count: number;
}

export interface PromoteWorkflowInput {
  batchId: number;
  title: string;
}

export interface Chapter {
  // ... 现有字段 ...
  source_kind: 'transformed' | 'original';
  source_chapter_id: number | null;
}
```

- [ ] **Step 2: 加 IPC wrappers**

在 `src/ipc/commands.ts`：

```typescript
export const promoteWorkflow = (input: PromoteWorkflowInput): Promise<DataAsset> =>
  invoke<DataAsset>('promote_workflow', input);

export const countPromotedDataAssetsByWorkflow = (batchId: number): Promise<number> =>
  invoke<number>('count_promoted_data_assets_by_workflow', { batchId });

export const listPromotedDataAssetsForWorkflow = (batchId: number): Promise<DataAsset[]> =>
  invoke<DataAsset[]>('list_promoted_data_assets_for_workflow', { batchId });

export const listDataAssetsByUpload = (uploadId: number): Promise<DataAsset[]> =>
  invoke<DataAsset[]>('list_data_assets_by_upload', { uploadId });
```

- [ ] **Step 3: 编译验证**

```bash
cd D:/Git/novel-style-converter
npx vite build 2>&1 | tail -15
```

期望：通过。

- [ ] **Step 4: Commit**

```bash
git add src/ipc/types.ts src/ipc/commands.ts
git commit -m "feat(ipc-ts): DataAssetKind/源/派生字段 + 4 个新 IPC wrapper"
```

---

## Task 10: 前端 stores 透传新字段

**Files:**
- Modify: `src/stores/dataAsset.ts`
- Modify: `src/stores/workflows.ts`

- [ ] **Step 1: dataAsset store 加 action**

```typescript
async function promoteWorkflow(batchId: number, title: string) {
  const newDa = await invoke<DataAsset>('promote_workflow', { batchId, title });
  await refresh();
  return newDa;
}
```

store 内部 `loadDataAssets()` 返回类型用 `DataAssetWithUpload[]`，新字段自动接收。

- [ ] **Step 2: workflows store 透传 promoted_count**

把 `WorkflowSummary` 字段直接透传，UI 层按 `row.promoted_count > 0` 决定是否显示 tag。

- [ ] **Step 3: 编译验证**

```bash
cd D:/Git/novel-style-converter
npx vite build 2>&1 | tail -15
```

期望：通过。

- [ ] **Step 4: Commit**

```bash
git add src/stores/dataAsset.ts src/stores/workflows.ts
git commit -m "feat(stores): 透传 kind/source_*/promoted_count + promoteWorkflow action"
```

---
## Task 11: 新建 PromoteWorkflowDialog 组件

**Files:**
- Create: `src/components/PromoteWorkflowDialog.vue`

- [ ] **Step 1: 写组件**

参考 `src/components/CreateBatchDialog.vue` 的结构。Props:

```typescript
const props = defineProps<{
  open: boolean;
  workflow: WorkflowSummary;
  sourceDataAssetTitle: string;
  successCount: number;
  failCount: number;
  skipCount: number;
}>();

const emit = defineEmits<{
  'update:open': [boolean];
  'confirm': [string];
}>();

const title = ref('');
const submitting = ref(false);

watch(() => props.open, (o) => {
  if (o) {
    const defaultLabel = props.workflow.label ?? `工作流 #${props.workflow.id}`;
    title.value = `${props.sourceDataAssetTitle} - ${defaultLabel}`;
  }
});

async function onConfirm() {
  if (!title.value.trim()) return;
  submitting.value = true;
  try {
    emit('confirm', title.value.trim());
  } finally {
    submitting.value = false;
  }
}
```

模板：

```vue
<Dialog :open="open" title="转为数据资产" @update:open="emit('update:open', $event)">
  <div class="promote-form">
    <label>
      数据资产标题 <span class="required">*</span>
      <input v-model="title" type="text" placeholder="输入标题" :disabled="submitting" />
    </label>
    <div class="summary">
      <p><span class="dot dot-success"></span>{{ successCount }} 章将使用转换结果</p>
      <p><span class="dot dot-original"></span>{{ failCount }} 章将使用原文</p>
      <p><span class="dot dot-original"></span>{{ skipCount }} 章被跳过,将使用原文</p>
    </div>
  </div>
  <template #footer>
    <Button size="small" @click="emit('update:open', false)">取消</Button>
    <Button size="small" kind="primary" :disabled="!title.trim()" :loading="submitting" @click="onConfirm">确认转正</Button>
  </template>
</Dialog>
```

样式按现有 Dialog/Button 风格。

- [ ] **Step 2: 编译验证**

```bash
cd D:/Git/novel-style-converter
npx vite build 2>&1 | tail -15
```

期望：通过。

- [ ] **Step 3: Commit**

```bash
git add src/components/PromoteWorkflowDialog.vue
git commit -m "feat(ui): PromoteWorkflowDialog 组件(必填 title + 成功/失败/跳过摘要)"
```

---

## Task 12: TransformationNovelDetail.vue 接通转正流程

**Files:**
- Modify: `src/views/TransformationNovelDetail.vue`

- [ ] **Step 1: 工作流 tab 列表行加 已转正 × N tag**

找到工作流 tab 表格渲染处，按 `row.promoted_count > 0` 显示 tag。

- [ ] **Step 2: 工作流详情弹窗顶部加"转为数据资产"按钮**

仅 `selectedWorkflow.status === 'stopped'` 时显示。

- [ ] **Step 3: 弹窗控制 + 调用 IPC**

```typescript
const promoteOpen = ref(false);
const promoteLoading = ref(false);
const promoteErr = ref('');

async function confirmPromote(title: string) {
  if (!selectedWorkflow.value) return;
  promoteLoading.value = true;
  promoteErr.value = '';
  try {
    await promoteWorkflow({ batchId: selectedWorkflow.value.id, title });
    await store.refreshWorkflows(tnId.value);
    promoteOpen.value = false;
  } catch (e: unknown) {
    promoteErr.value = e instanceof Error ? e.message : String(e);
  } finally {
    promoteLoading.value = false;
  }
}
```

- [ ] **Step 4: 编译 + 手工验证**

```bash
cd D:/Git/novel-style-converter
npx vite build 2>&1 | tail -10
npm run tauri dev
```

期望：UI 显示"▶ 转为数据资产"按钮 + 弹窗 + 转正后行显示"已转正 × 1"。

- [ ] **Step 5: Commit**

```bash
git add src/views/TransformationNovelDetail.vue
git commit -m "feat(ui): 工作流 tab 已转正 tag + 转正按钮 + 弹窗接通"
```

---

## Task 13: Library.vue 加类型/派生数列

**Files:**
- Modify: `src/views/Library.vue`

- [ ] **Step 1: 加列定义**

```typescript
{
  key: 'kind',
  title: '类型',
  width: 140,
  render: (row) => row.kind === 'promoted'
    ? html`<Tag kind="success">派生</Tag><a href="#" @click.prevent="goWorkflow(row.source_workflow_id!)">工作流 #${row.source_workflow_id}</a>`
    : html`<Tag>源</Tag>`,
},
{
  key: 'promoted_count',
  title: '派生数',
  width: 100,
  render: (row) => row.promoted_count > 0 ? `${row.promoted_count} 个` : '—',
},
```

- [ ] **Step 2: 行点击按 kind 分流**

- kind=source → 跳 Parse 页
- kind=promoted → 跳 workflow 详情（`source_workflow_id` 通过 JOIN batch.transformation_novel_id 拿）

具体 tn_id 拿法：`SELECT tn.id FROM batches b JOIN transformation_novels tn ON b.transformation_novel_id = tn.id WHERE b.id = ?`，或是在 `list_with_upload` JOIN 一次性带出。最简方案：在 DataAssetWithUpload 里加 `tn_id: Option<i64>` 字段，从 batches JOIN 拿。

- [ ] **Step 3: 编译验证**

```bash
cd D:/Git/novel-style-converter
npx vite build 2>&1 | tail -10
```

期望：通过。

- [ ] **Step 4: Commit**

```bash
git add src/views/Library.vue
git commit -m "feat(ui): Library 加类型/派生数列 + 行点击按 kind 分流"
```

---

## Task 14: Upload.vue 加派生数列

**Files:**
- Modify: `src/views/Upload.vue`
- Modify: `src/views/Library.vue`

- [ ] **Step 1: Upload 加列**

```typescript
{
  key: 'promoted_count',
  title: '派生',
  width: 100,
  render: (row) => row.promoted_count > 0
    ? html`<a href="#" @click.prevent="goLibraryByUpload(row.id)">${row.promoted_count} 个</a>`
    : '—',
},
```

`goLibraryByUpload(uploadId)` 跳 Library 带过滤：

```typescript
router.push({ name: 'library', query: { uploadId: String(uploadId) } });
```

- [ ] **Step 2: Library 支持 uploadId query**

在 Library.vue 初始化时读 `route.query.uploadId`，过滤列表。

- [ ] **Step 3: 编译验证**

```bash
cd D:/Git/novel-style-converter
npx vite build 2>&1 | tail -10
```

期望：通过。

- [ ] **Step 4: Commit**

```bash
git add src/views/Upload.vue src/views/Library.vue
git commit -m "feat(ui): Upload 加派生数列 + Library 支持 uploadId 过滤跳转"
```

---

## Task 15: parse.vue 接 promoted da 只读模式 + source_kind 列

**Files:**
- Modify: `src/views/parse.vue`

- [ ] **Step 1: 按 kind 分流**

进入 parse.vue 时先 `await getDataAsset(daId)`，拿到 `kind`：

- kind=source → 现有行为
- kind=promoted → 只读模式

```typescript
const da = ref<DataAsset | null>(null);
const isPromoted = computed(() => da.value?.kind === 'promoted');

onMounted(async () => {
  da.value = await getDataAsset(daId);
});
```

- [ ] **Step 2: 章节列表加 source_kind 列**

```vue
<Tag v-if="ch.source_kind === 'transformed'" kind="success">转换</Tag>
<Tag v-else kind="muted">原文</Tag>
```

- [ ] **Step 3: 只读模式隐藏"重新解析"按钮**

```vue
<Button v-if="!isPromoted" size="small" @click="...">重新解析</Button>
```

- [ ] **Step 4: 编译验证**

```bash
cd D:/Git/novel-style-converter
npx vite build 2>&1 | tail -10
```

期望：通过。

- [ ] **Step 5: Commit**

```bash
git add src/views/parse.vue
git commit -m "feat(ui): Parse 页支持 promoted 只读模式 + source_kind 列"
```

---

## Task 16: 全量验证

- [ ] **Step 1: Rust 测试**

```bash
cd D:/Git/novel-style-converter
cargo test -p nsc-core --lib 2>&1 | tail -10
```

期望：22 + 新增 5 = 27 测试全 pass。

- [ ] **Step 2: Rust check**

```bash
cd D:/Git/novel-style-converter
cargo check --workspace 2>&1 | tail -10
```

期望：无 error/warning。

- [ ] **Step 3: 前端 build**

```bash
cd D:/Git/novel-style-converter
npx vite build 2>&1 | tail -10
```

期望：通过。

- [ ] **Step 4: 手工黄金路径**

```bash
cd D:/Git/novel-style-converter
npm run tauri dev
```

1. 上传文件 → 解析章节
2. 创建 TN
3. 创建 workflow → 等 Stopped
4. 工作流详情点"转为数据资产" → 弹窗填 title → 确认
5. Library 看到新"派生"行 + Parse 页能读 body
6. 工作流 tab 行显示"已转正 × 1" tag
7. 上传页行显示"派生 1 个 da"
8. 二次转正同 workflow → 第 2 个 promoted da
9. 删 promoted da → 列表消失 + 计数 -1

- [ ] **Step 5: Commit 验证记录**

```bash
git log --oneline -10
git status --short
```

期望：working tree 干净，所有改动已 commit。

---

## Self-Review

- [x] **Spec 覆盖**:
  - §2.1 数据模型扩展 → Task 1, 2, 3, 5
  - §3.3 填充规则 → Task 4, 5
  - §5.1 promote_workflow 单事务 → Task 4
  - §5.3 派生计数 → Task 6, 7
  - §6 IPC 边界 → Task 8, 9
  - §7.1 工作流详情页 → Task 11, 12
  - §7.2 数据资产页 → Task 13
  - §7.3 上传页 → Task 14
  - §7.4 Parse 页 → Task 15
  - §9 测试 → Task 16
- [x] **占位符扫描**: 无 TBD/TODO/未完成段落
- [x] **类型一致性**:
  - `DataAsset` 字段：`id, upload_id, title, parsed_at, source_filename, kind, source_workflow_id, source_data_asset_id, note`
  - `WorkflowSummary` 字段新增 `promoted_count: i64`
  - `Chapter` 字段新增 `source_kind: String, source_chapter_id: Option<i64>`
  - `DataAssetWithUpload` 字段含上述 + `filename, byte_size, word_count, tn_count, promoted_count`
- [x] **TDD 顺序**: Task 4 用 TDD（写测试 → 跑看失败 → 实现 → 通过）；其他 task 依赖 Rust 编译反馈
- [x] **commit 粒度**: 16 个 task ≈ 16 个 commit，便于回滚