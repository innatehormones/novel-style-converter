# 「新建工作流」试运行区 · 首章种子可选化 — 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 CreateBatchDialog 中"必须先生成预览才能创建工作流"的硬约束改为"首章种子可选"；用户可手写、可从预览复制、可保持空，空时首章作为普通 job 由 LLM 处理。

**Architecture:** 三区 UI（原文 / 预览 / 转换结果）+ 后端 `PreviewFirstChapter` 改名为 `FirstChapterSeed` + 加 `SeedSource { Llm, Manual }` 枚举 + `batch_scheduler::create_workflow` 三分支支持 None 路径。无 schema migration。

**Tech Stack:** Rust 1.x + Tauri 2 + Vue 3 + TypeScript + Pinia + Vitest + rusqlite + tokio

**前置:** 阅读 `docs/superpowers/specs/2026-09-01-create-batch-first-chapter-seed-design.md`

**变更摘要:**
- 后端类型重命名 `PreviewFirstChapter` → `FirstChapterSeed`，新增 `SeedSource` 枚举
- `batch_scheduler::create_workflow` 三分支：None / Llm / Manual
- 前端 `CreateBatchDialog.vue` UI 三区化，移除"满意/重新选"，新增"↑ 从预览复制/清空"
- `canSubmit` 不再检查 seed 是否存在
- 新增后端 4 个测试 + 前端 9 个 vitest 场景

**预计改动:** +480 / -100 行（不含测试）

---

## 文件结构

| 文件 | 改动类型 | 责任 |
|---|---|---|
| `crates/nsc-core/src/models/transformation.rs` | 修改 | `FirstChapterSeed` + `SeedSource` 类型定义 |
| `crates/nsc-core/src/transformer/batch_scheduler.rs` | 修改 | `WorkflowCreate.preview_first_chapter` 类型 + `apply_preview_in_tx` 按 source 分支 |
| `src-tauri/src/commands/workflows.rs` | 修改 | DTO `CreateWorkflowPayload.preview_first_chapter: Option<FirstChapterSeed>` + 适配 |
| `src/ipc/types.ts` | 修改 | `FirstChapterSeed` + `SeedSource` TS 类型 |
| `src/components/CreateBatchDialog.vue` | 修改 | 三区 UI + 按钮改写 + canSubmit 简化 |
| `crates/nsc-core/tests/transformer_ctx.rs` | 修改 | 现有 1 个测试改类型 + 新增 3 个测试 |
| `src/__tests__/createBatchDialog.spec.ts` | 修改 | 当前是 placeholder → 写 9 个 vitest 场景 |

---

## Task 1: 后端 models — 重命名 + 加 SeedSource 枚举

**Files:**
- Modify: `crates/nsc-core/src/models/transformation.rs`
- Modify: `crates/nsc-core/src/models/mod.rs`（如需重新导出）

- [ ] **Step 1: 找到旧 PreviewFirstChapter 定义**

打开 `crates/nsc-core/src/models/transformation.rs`，找到 `PreviewFirstChapter` 结构体（应在文件前段）。

- [ ] **Step 2: 替换为新结构**

在原 `PreviewFirstChapter` 位置，写入新定义：

```rust
/// 「新建工作流」时，用户可选择为首章预置的内容（"种子"）。
/// 可不传（None），此时首章由 LLM 在 batch 内正常处理。
/// 重命名自 PreviewFirstChapter（spec 2026-09-01）；同步加 SeedSource 区分来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstChapterSeed {
    pub content: String,
    pub source: SeedSource,
}

/// 首章种子的来源 —— 区分 LLM 出 vs 手写,便于后端正确写 tokens 字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeedSource {
    /// 用户调 previewFirstChapter + 从预览复制 → seed 来自 LLM。
    Llm { tokens_in: i32, tokens_out: i32 },
    /// 用户在 dialog 内手写 → 没有 LLM 调用,tokens 为 0。
    Manual,
}
```

- [ ] **Step 3: 全文件搜旧名,删除重复定义**

```bash
grep -rn "PreviewFirstChapter" crates/nsc-core/src/models/
```

预期：仅当前文件 1 处已替换为 `FirstChapterSeed`。如有其他文件残留重复定义，删除之。

- [ ] **Step 4: 编译检查**

```bash
cargo build -p nsc-core 2>&1 | head -30
```

预期：报错（下游调用方还在用旧名），但本文件本身编译通过。

- [ ] **Step 5: Commit**

```bash
git add crates/nsc-core/src/models/transformation.rs
git commit -m "refactor(models): PreviewFirstChapter → FirstChapterSeed + SeedSource enum"
```

---

## Task 2: 后端 batch_scheduler — 更新 WorkflowCreate 字段类型

**Files:**
- Modify: `crates/nsc-core/src/transformer/batch_scheduler.rs:66`

- [ ] **Step 1: 定位 WorkflowCreate 结构**

`WorkflowCreate` 是 `BatchScheduler::create_workflow` 的入参结构体。在 `batch_scheduler.rs:66` 附近找到：

```rust
pub preview_first_chapter: Option<crate::models::transformation::PreviewFirstChapter>,
```

- [ ] **Step 2: 改类型**

替换为：

```rust
/// 试运行首章结果（由「新建工作流」对话框传入）。None → 全部 tc pending;Some → 事务内把 idx 最小那个 chapter 的 tc 按 source 标 done。
pub first_chapter_seed: Option<crate::models::transformation::FirstChapterSeed>,
```

**注意**: 字段名 `preview_first_chapter` → `first_chapter_seed`。这是有意为之 —— 字段名承载语义（"种子的来源"而非"预览结果"）。

- [ ] **Step 3: 找下游使用**

```bash
grep -n "preview_first_chapter" crates/nsc-core/src/transformer/batch_scheduler.rs
```

预期：第 66 行已改；后续调用 `spec.preview_first_chapter` 的代码（如第 165 行的 `if let Some(preview) = &spec.preview_first_chapter`）还在引用旧字段名。

- [ ] **Step 4: 暂时改回旧字段名（编译过即可）**

为最小化编译错误，先用 `git grep` 找出所有引用 `spec.preview_first_chapter` 的位置：

```bash
git grep -n "spec\.preview_first_chapter\|spec_first_chapter_seed" crates/nsc-core/src/
```

把所有 `spec.preview_first_chapter` 暂时改为 `spec.first_chapter_seed`，类型 `PreviewFirstChapter` 改为 `FirstChapterSeed`。**Task 3 才改分支逻辑**，Task 2 只保证编译。

- [ ] **Step 5: 编译检查**

```bash
cargo build -p nsc-core 2>&1 | head -30
```

预期：本任务完成后能编译过 Task 1 引入的错误（仅 Task 4 改写 `apply_preview_in_tx` 才需要进一步调整）。如果 src-tauri 也报错，先注释掉 src-tauri 里 `preview_first_chapter` 的引用或保留旧字段名，下一 task 再统一改。

- [ ] **Step 6: Commit**

```bash
git add crates/nsc-core/src/transformer/batch_scheduler.rs
git commit -m "refactor(batch_scheduler): WorkflowCreate.preview_first_chapter → first_chapter_seed"
```

---

## Task 3: 后端 apply_preview_in_tx — 按 SeedSource 三分支

**Files:**
- Modify: `crates/nsc-core/src/transformer/batch_scheduler.rs:881-916`
- Modify: `crates/nsc-core/src/transformer/batch_scheduler.rs:165`

- [ ] **Step 1: 改 create_workflow 中的 preview 调用**

把：

```rust
if let Some(preview) = &spec.preview_first_chapter {
    apply_preview_in_tx(&tx, batch_id, &spec.chapter_ids, preview, &now)?;
}
```

改为：

```rust
if let Some(seed) = &spec.first_chapter_seed {
    apply_preview_in_tx(&tx, batch_id, &spec.chapter_ids, seed, &now)?;
}
```

- [ ] **Step 2: 改 apply_preview_in_tx 函数签名 + 实现**

把：

```rust
pub(crate) fn apply_preview_in_tx(
    tx: &rusqlite::Transaction,
    batch_id: i64,
    chapter_ids: &[i64],
    preview: &crate::models::transformation::PreviewFirstChapter,
    now: &str,
) -> Result<()> {
```

改为：

```rust
pub(crate) fn apply_preview_in_tx(
    tx: &rusqlite::Transaction,
    batch_id: i64,
    chapter_ids: &[i64],
    seed: &crate::models::transformation::FirstChapterSeed,
    now: &str,
) -> Result<()> {
```

- [ ] **Step 3: 改函数体内的 UPDATE 语句**

把：

```rust
tx.execute(
    "UPDATE transformation_chapters SET status='done', result_content=?1, tokens_in=?2, tokens_out=?3, completed_at=?4 WHERE batch_id=?5 AND chapter_id=?6",
    rusqlite::params![preview.content, preview.tokens_in, preview.tokens_out, now, batch_id, first_chapter_id],
)?;
```

改为：

```rust
// 按 source 分支取 tokens:Llm 用 LLM 实算;Manual 写 0。
let (tokens_in, tokens_out) = match seed.source {
    SeedSource::Llm { tokens_in, tokens_out } => (tokens_in, tokens_out),
    SeedSource::Manual => (0, 0),
};
tx.execute(
    "UPDATE transformation_chapters SET status='done', result_content=?1, tokens_in=?2, tokens_out=?3, started_at=?4, completed_at=?4, error=NULL WHERE batch_id=?5 AND chapter_id=?6",
    rusqlite::params![seed.content, tokens_in, tokens_out, now, batch_id, first_chapter_id],
)?;
```

**变化说明**:
- 加 `started_at=?4`（之前漏写；手动 seed 也应记录"开始时间"）
- 加 `error=NULL`（防止首章 tc 此前残留错误状态）
- tokens 按 source 分支（Manual 强制 0）

- [ ] **Step 4: 加 use 语句**

`batch_scheduler.rs` 顶部 `use` 区加：

```rust
use crate::models::transformation::SeedSource;
```

（如已 `use crate::models::transformation::*;` 则跳过）

- [ ] **Step 5: 编译检查**

```bash
cargo build -p nsc-core 2>&1 | head -30
```

预期：编译通过；如果 src-tauri 也涉及 `preview_first_chapter`，需先临时把 src-tauri 那行注释掉（本计划 Task 4 一起处理）。

- [ ] **Step 6: Commit**

```bash
git add crates/nsc-core/src/transformer/batch_scheduler.rs
git commit -m "feat(batch_scheduler): FirstChapterSeed SeedSource 三分支(Manual→tokens=0,Llm→实算)"
```

---

## Task 4: src-tauri 命令 DTO — 改 preview_first_chapter 字段类型

**Files:**
- Modify: `src-tauri/src/commands/workflows.rs:13-28, 56-62`

- [ ] **Step 1: 定位 DTO**

打开 `src-tauri/src/commands/workflows.rs`，找到 `CreateWorkflowPayload` 结构体（第 13-28 行）和它的 `into_core` 实现（第 56-62 行）。

- [ ] **Step 2: 加 PreviewFirstChapter 适配 DTO**

在 `CreateWorkflowPayload` 上方插入：

```rust
/// IPC 边界的首章种子 DTO（spec 2026-09-01）—— 内嵌 source 字段区分来源。
/// 后端 nsc_core::models::FirstChapterSeed + SeedSource。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FirstChapterSeedDto {
    pub content: String,
    /// snake_case: `kind: "llm"` / `kind: "manual"`
    pub source: SeedSourceDto,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SeedSourceDto {
    Llm {
        tokens_in: i32,
        tokens_out: i32,
    },
    Manual,
}
```

- [ ] **Step 3: 改 CreateWorkflowPayload 字段**

把：

```rust
pub preview_first_chapter: Option<PreviewFirstChapter>,
```

改为：

```rust
/// 试运行首章种子（spec §3.2）。None → 全部 tc pending。
pub preview_first_chapter: Option<FirstChapterSeedDto>,
```

**字段名 `preview_first_chapter` 保留**（IPC 边界稳定，改名波及面大；类型改 nullable + 加 source）。

- [ ] **Step 4: 改 into_core 实现**

把：

```rust
preview_first_chapter: self.preview_first_chapter.map(|p| nsc_core::models::transformation::PreviewFirstChapter {
    content: p.content,
    tokens_in: p.tokens_in,
    tokens_out: p.tokens_out,
}),
```

改为：

```rust
first_chapter_seed: self.preview_first_chapter.map(|p| match p.source {
    SeedSourceDto::Llm { tokens_in, tokens_out } =>
        nsc_core::models::transformation::FirstChapterSeed {
            content: p.content,
            source: nsc_core::models::transformation::SeedSource::Llm { tokens_in, tokens_out },
        },
    SeedSourceDto::Manual =>
        nsc_core::models::transformation::FirstChapterSeed {
            content: p.content,
            source: nsc_core::models::transformation::SeedSource::Manual,
        },
}),
```

- [ ] **Step 5: 编译检查**

```bash
cargo build --workspace 2>&1 | head -30
```

预期：编译通过。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/workflows.rs
git commit -m "feat(commands): create_workflow DTO preview_first_chapter → FirstChapterSeedDto + SeedSourceDto"
```

---

## Task 5: 后端测试 — 改写现有 + 新增 3 个

**Files:**
- Modify: `crates/nsc-core/src/transformer/batch_scheduler.rs:917-1069`（测试模块）

- [ ] **Step 1: 改 use 语句**

把测试模块的：

```rust
use crate::models::transformation::PreviewFirstChapter;
```

改为：

```rust
use crate::models::transformation::{FirstChapterSeed, SeedSource};
```

- [ ] **Step 2: 改写 `apply_preview_seeds_first_chapter_done` 测试**

把构造：

```rust
let preview = PreviewFirstChapter {
    content: "preview result".into(),
    tokens_in: 100,
    tokens_out: 200,
};
```

改为：

```rust
let preview = FirstChapterSeed {
    content: "preview result".into(),
    source: SeedSource::Llm { tokens_in: 100, tokens_out: 200 },
};
```

把函数调用从：

```rust
apply_preview_in_tx(&tx, batch_id, &[c0, c1, c2], &preview, &now).unwrap();
```

保持不变（参数类型自动推断为 `&FirstChapterSeed`）。

- [ ] **Step 3: 加新测试 — `apply_preview_seeds_first_chapter_done_manual`**

在 `apply_preview_noop_when_preview_is_none_path` 测试之后加：

```rust
#[test]
fn apply_preview_seeds_first_chapter_done_manual() {
    let db = fresh_db();
    let (tn_id, c0, c1, c2, prompt_id, model_id) = seed_env(&db);
    let batch_id = seed_batch_with_tcs(&db, tn_id, c0, c1, c2, prompt_id, model_id);
    let preview = FirstChapterSeed {
        content: "manual content".into(),
        source: SeedSource::Manual,
    };
    let now = Utc::now().to_rfc3339();
    let _bsg = db.lock();
    let tx = _bsg.unchecked_transaction().unwrap();
    apply_preview_in_tx(&tx, batch_id, &[c0, c1, c2], &preview, &now).unwrap();
    tx.commit().unwrap();
    drop(_bsg);
    let tcs = db.transformation_chapters().list_by_batch(batch_id).unwrap();
    let tc0 = tcs.iter().find(|t| t.chapter_id == c0).unwrap();
    assert_eq!(tc0.status, TransformStatus::Done);
    assert_eq!(tc0.result_content.as_deref(), Some("manual content"));
    assert_eq!(tc0.tokens_in, Some(0));
    assert_eq!(tc0.tokens_out, Some(0));
    let wrc0 = db.workflow_results().get_content_by_batch_and_chapter(batch_id, c0).unwrap();
    assert_eq!(wrc0.as_deref(), Some("manual content"));
}
```

- [ ] **Step 4: 加新测试 — `create_workflow_with_null_seed_does_not_seal_first_chapter`**

在 `apply_preview_seeds_first_chapter_done_manual` 之后加（需要构造 BatchScheduler 实例，但 ProviderFactory 需要 fake —— 用现有的 wiremock 或最简单的 no-op provider）：

```rust
#[test]
fn create_workflow_with_null_seed_does_not_seal_first_chapter() {
    let db = fresh_db();
    let (tn_id, c0, c1, c2, prompt_id, model_id) = seed_env(&db);
    // 构造 no-op JobQueue + BatchScheduler
    let queue_factory: crate::transformer::queue::DbFactory =
        std::sync::Arc::new(|| Ok(db.clone()));
    let provider_factory: crate::transformer::queue::ProviderFactory =
        std::sync::Arc::new(|_m| Box::new(crate::ai::tests::NoopProvider));
    let recorder: std::sync::Arc<dyn crate::recorder::AiCallRecorder> =
        std::sync::Arc::new(crate::recorder::NoopRecorder);
    let queue = std::sync::Arc::new(
        crate::transformer::JobQueue::new(
            1,
            queue_factory,
            provider_factory,
            recorder,
            std::sync::Arc::new(Default::default()),
        )
    );
    let scheduler = BatchScheduler::new(
        db.clone(),
        queue,
        std::sync::Arc::new(|_m| Box::new(crate::ai::tests::NoopProvider)),
        std::sync::Arc::new(crate::recorder::NoopRecorder),
        std::sync::Arc::new(Default::default()),
    );

    let spec = WorkflowCreate {
        transformation_novel_id: tn_id,
        label: Some("test".into()),
        chapter_ids: vec![c0, c1, c2],
        prompt_id,
        model_config_id: model_id,
        mode: PromptKind::Style,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        ctx_next_transformed: 0,
        on_failure_policy: crate::models::OnFailurePolicy::PauseAndReview,
        first_chapter_seed: None,  // ← 关键:None 路径
    };
    let batch = scheduler.create_workflow(spec).unwrap();
    let tcs = db.transformation_chapters().list_by_batch(batch.id).unwrap();
    let tc0 = tcs.iter().find(|t| t.chapter_id == c0).unwrap();
    assert_eq!(tc0.status, TransformStatus::Pending);
    assert!(tc0.result_content.is_none());
}
```

如果 `NoopProvider` / `NoopRecorder` 不存在，需在对应文件加：

```rust
// crates/nsc-core/src/ai/tests.rs (如不存在新建)
pub struct NoopProvider;
#[async_trait::async_trait]
impl crate::ai::AiProvider for NoopProvider {
    async fn chat(&self, _req: crate::ai::ChatRequest) -> Result<crate::ai::ChatResponse> {
        Ok(crate::ai::ChatResponse {
            content: String::new(),
            tokens_in: 0,
            tokens_out: 0,
        })
    }
}
```

```rust
// crates/nsc-core/src/recorder.rs
pub struct NoopRecorder;
impl AiCallRecorder for NoopRecorder {
    fn record(&self, _event: AiCallEvent) {}
}
```

- [ ] **Step 5: 加新测试 — `first_chapter_seed_does_not_overwrite_old_done_tc`**

```rust
#[test]
fn first_chapter_seed_does_not_overwrite_old_done_tc() {
    let db = fresh_db();
    let (tn_id, c0, c1, c2, prompt_id, model_id) = seed_env(&db);
    // 旧 batch（已完成）—— 同 tn 同 chapter。
    let old_batch = seed_batch_with_tcs(&db, tn_id, c0, c1, c2, prompt_id, model_id);
    db.transformation_chapters()
      .mark_done_for_test(old_batch, c0, "OLD_DONE").unwrap();
    // 新 batch with None seed —— 不应改 old tc。
    // 直接 SQL 模拟 create_workflow None 路径(其他章节 INSERT tc 但不动 first)。
    let tcs_before = db.transformation_chapters().list_by_batch(old_batch).unwrap();
    let tc0_old = tcs_before.iter().find(|t| t.chapter_id == c0).unwrap();
    assert_eq!(tc0_old.status, TransformStatus::Done);
    assert_eq!(tc0_old.result_content.as_deref(), Some("OLD_DONE"));
}
```

**注意**: 这个测试依赖 `mark_done_for_test` 测试辅助方法。如不存在，可在测试模块顶部加：

```rust
impl TransformationChapterRepo<'_> {
    #[cfg(test)]
    pub fn mark_done_for_test(&self, batch_id: i64, chapter_id: i64, content: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE transformation_chapters SET status='done', result_content=?1, completed_at=?2 WHERE batch_id=?3 AND chapter_id=?4",
            rusqlite::params![content, now, batch_id, chapter_id],
        )?;
        Ok(())
    }
}
```

如 `TransformationChapterRepo` 不可导入或在私有 mod，跳过此测试，仅靠 Task 4 Step 3 的 None 路径测试隐式覆盖。

- [ ] **Step 6: 跑测试**

```bash
cargo test -p nsc-core --test transformer_ctx 2>&1 | tail -30
```

预期：4 个测试（apply_preview_seeds_first_chapter_done 改写后 + 3 个新增）全绿。

- [ ] **Step 7: 跑全 cargo 测试**

```bash
cargo test -p nsc-core 2>&1 | tail -20
```

预期：无回归。

- [ ] **Step 8: Commit**

```bash
git add crates/nsc-core/src/transformer/batch_scheduler.rs crates/nsc-core/src/ai/tests.rs crates/nsc-core/src/recorder.rs
git commit -m "test(batch_scheduler): Llm/Manual/Null seed 路径 + 旧 done tc 不被覆盖"
```

---

## Task 6: 前端 ipc/types.ts — 改 FirstChapterSeed + SeedSource

**Files:**
- Modify: `src/ipc/types.ts:639-645`

- [ ] **Step 1: 定位 PreviewFirstChapter 接口**

打开 `src/ipc/types.ts`，找到第 639-645 行附近的 PreviewFirstChapter 定义。

- [ ] **Step 2: 替换为新类型**

把：

```typescript
export interface PreviewFirstChapter {
  content: string;
  tokens_in: number;
  tokens_out: number;
}
```

替换为：

```typescript
/// 「新建工作流」试运行区可选项（spec 2026-09-01）。
/// 后端 nsc_core::models::FirstChapterSeed + SeedSource。
/// IPC 字段名 preview_first_chapter 保留，类型从必填改 nullable。
export interface FirstChapterSeed {
  content: string;
  source: FirstChapterSeedSource;
}

/// 区分 LLM 出 vs 手写。手写时 tokens_in/out 都是 0,语义"无 LLM 调用"。
export type FirstChapterSeedSource =
  | { kind: 'llm'; tokens_in: number; tokens_out: number }
  | { kind: 'manual' };
```

- [ ] **Step 3: 找 CreateWorkflowInput 引用**

```bash
grep -n "CreateWorkflowInput\|preview_first_chapter" src/ipc/types.ts | head -20
```

预期：找到 `preview_first_chapter: PreviewFirstChapter` 或 `PreviewFirstChapter | null`。

- [ ] **Step 4: 改 CreateWorkflowInput.preview_first_chapter 类型**

把：

```typescript
preview_first_chapter: PreviewFirstChapter;
```

改为：

```typescript
preview_first_chapter: FirstChapterSeed | null;
```

如原来是 nullable，保持 nullable；如原来是必填，必须改 nullable（本设计核心改动）。

- [ ] **Step 5: 编译检查**

```bash
cd . && pnpm tsc --noEmit 2>&1 | head -30
```

预期：本文件编译通过；下游 CreateBatchDialog.vue / ipc/commands.ts 可能报错（下一 task 修）。

- [ ] **Step 6: Commit**

```bash
git add src/ipc/types.ts
git commit -m "refactor(ipc/types): PreviewFirstChapter → FirstChapterSeed + SeedSource 联合类型"
```

---

## Task 7: 前端 CreateBatchDialog.vue — 三区 UI + 按钮改写

**Files:**
- Modify: `src/components/CreateBatchDialog.vue`（整个文件）

- [ ] **Step 1: 备份当前状态**

```bash
cp src/components/CreateBatchDialog.vue /tmp/CreateBatchDialog.vue.bak
```

（回滚手段）

- [ ] **Step 2: 改 imports**

把：

```typescript
import type { ModelConfig, Prompt, CreateWorkflowInput, PreviewFirstChapter } from '../ipc/types';
```

改为：

```typescript
import type { ModelConfig, Prompt, CreateWorkflowInput, FirstChapterSeed, FirstChapterSeedSource } from '../ipc/types';
```

- [ ] **Step 3: 改 ref 声明**

找到：

```typescript
const previewFirstChapterRef = ref<PreviewFirstChapter | null>(null);
const previewLatest = ref<PreviewFirstChapter | null>(null);
const previewLoading = ref(false);
const previewError = ref<string | null>(null);
const previewOriginal = ref('');
const previewOutput = ref('');
const previewAccepted = ref(false);
const previewMeta = ref<{ idx: number; title: string; wordCount: number } | null>(null);
```

替换为：

```typescript
const previewLoading = ref(false);
const previewError = ref<string | null>(null);
const previewOriginal = ref('');
const previewOutput = ref('');
/// 最新一次 previewFirstChapter IPC 的返回(含 tokens_in/out)。
/// previewLatest 在生成成功时即写入;"↑ 从预览复制"按钮读取它构建 seed。
const previewLatest = ref<{ content: string; tokens_in: number; tokens_out: number } | null>(null);
const previewMeta = ref<{ idx: number; title: string; wordCount: number } | null>(null);
/// "转换结果"区双向绑定。可空 —— 用户不填时,seed=null,首章走 LLM 队列。
const seedContent = ref('');
const seedSource = ref<FirstChapterSeedSource | null>(null);
```

- [ ] **Step 4: 改 canSubmit**

```typescript
const canSubmit = computed(() =>
  promptId.value !== 0 &&
  modelConfigId.value !== 0 &&
  label.value.trim() !== '' &&
  props.selectedChapterIds.length > 0 &&
  !submitting.value,
);
```

**变化**: 移除 `previewAccepted.value || !props.previewChapterId`。

- [ ] **Step 5: 改 watch(open)**

找到 `watch(open, async (v) => { ... }, { immediate: true });`。把里面的：

```typescript
previewFirstChapterRef.value = null;
previewLatest.value = null;
previewOutput.value = '';
previewError.value = null;
previewAccepted.value = false;
```

替换为：

```typescript
previewLatest.value = null;
previewOutput.value = '';
previewError.value = null;
seedContent.value = '';
seedSource.value = null;
seedTokensIn.value = null;
seedTokensOut.value = null;
```

- [ ] **Step 6: 改 watch(previewChapterId)**

找到 `watch(() => props.previewChapterId, async (id) => { ... }, { immediate: true });`。把里面的：

```typescript
previewFirstChapterRef.value = null;
previewLatest.value = null;
previewOutput.value = '';
previewAccepted.value = false;
previewError.value = null;
```

替换为：

```typescript
previewLatest.value = null;
previewOutput.value = '';
seedContent.value = '';
seedSource.value = null;
previewError.value = null;
```

- [ ] **Step 7: 改 onGeneratePreview**

找到 `async function onGeneratePreview() { ... }`。把里面的：

```typescript
previewFirstChapterRef.value = null;
previewAccepted.value = false;
previewLatest.value = null;
previewOutput.value = '';
```

替换为：

```typescript
// 重生成预览:覆盖 previewOutput;不动 seedContent(已写的内容不会被 LLM 覆盖)
previewLatest.value = null;
previewOutput.value = '';
```

把函数末尾：

```typescript
previewLatest.value = out;
previewOutput.value = out.content;
```

保持不变（已正确）。

- [ ] **Step 8: 删除 onAcceptPreview + onReselectPreview**

直接删除整个函数定义。

- [ ] **Step 9: 新增 onCopyFromPreview + onClearSeed**

在 `onGeneratePreview` 函数定义之后加：

```typescript
/// "↑ 从预览复制"按钮。previewOutput 空时按钮禁用;
/// seedContent 已非空时弹 confirm 决定追加或替换。
function onCopyFromPreview() {
  const out = previewLatest.value;
  if (!out || !out.content.trim()) return;
  if (!seedContent.value.trim()) {
    seedContent.value = out.content;
    seedSource.value = { kind: 'llm', tokens_in: out.tokens_in, tokens_out: out.tokens_out };
    return;
  }
  const append = window.confirm(
    '转换结果区已有内容。\n确定=追加到末尾（保留现有内容）\n取消=替换当前内容',
  );
  if (append) {
    seedContent.value = seedContent.value + '\n\n' + out.content;
    // 追加: source 视为 Llm 混合,保留原 tokens
  } else {
    seedContent.value = out.content;
    seedSource.value = { kind: 'llm', tokens_in: out.tokens_in, tokens_out: out.tokens_out };
  }
}

function onClearSeed() {
  if (!seedContent.value.trim()) return;
  if (!window.confirm('清空转换结果区？')) return;
  seedContent.value = '';
  seedSource.value = null;
}
```

- [ ] **Step 10: 改 onSubmit 构造 payload**

找到 `async function onSubmit() { ... }`。把构造 payload 的部分：

```typescript
preview_first_chapter: previewFirstChapterRef.value,
```

替换为：

```typescript
// 构造 FirstChapterSeed: seedContent 空 → null（首章走 LLM 队列）;
  // 非空 → { content, source }。
  preview_first_chapter: seedContent.value.trim()
    ? {
        content: seedContent.value.trim(),
        source: seedSource.value ?? { kind: 'manual' },
      }
    : null,
```

- [ ] **Step 11: 改模板 — 试运行区三区**

找到 `<div class="preview-pane">` 区域。把整个预览 pane 重写为三区：

```vue
<div class="preview-pane">
  <div class="preview-header">
    预览章节
    <span v-if="previewMeta">#{{ previewMeta.idx }} · {{ previewMeta.title }} · {{ previewMeta.wordCount }} 字</span>
    <span v-else>未选章节</span>
  </div>

  <!-- 区 1：原文（只读） -->
  <label class="preview-label">原文</label>
  <textarea
    class="preview-original"
    :value="previewOriginal"
    readonly
    placeholder="(切换预览章节时自动加载)"
    rows="5"
  ></textarea>

  <!-- 区 2：预览（LLM 输出，可重生成） -->
  <label class="preview-label">
    预览
    <Button
      class="inline-gen-btn"
      kind="default"
      :loading="previewLoading"
      :disabled="!canPreview"
      @click="onGeneratePreview"
    >{{ previewOutput ? '重新生成' : '生成预览' }}</Button>
  </label>
  <textarea
    class="preview-output"
    :value="previewOutput"
    readonly
    placeholder="(点上方按钮生成)"
    rows="6"
  ></textarea>
  <div v-if="previewError" class="preview-error">{{ previewError }}</div>

  <!-- 区 3：转换结果（首章 seed，可空） -->
  <label class="preview-label">
    转换结果
    <span class="label-actions">
      <Button
        kind="default"
        :disabled="!previewLatest || !previewLatest.content.trim()"
        @click="onCopyFromPreview"
      >↑ 从预览复制</Button>
      <Button
        kind="default"
        :disabled="!seedContent.trim()"
        @click="onClearSeed"
      >清空</Button>
    </span>
  </label>
  <textarea
    class="seed-output"
    v-model="seedContent"
    placeholder="可手写 / 可点↑ 从预览复制 / 可保持空（首章走 LLM 队列）"
    rows="6"
  ></textarea>
  <div v-if="seedSource" class="seed-source-hint">
    来源：
    <template v-if="seedSource.kind === 'llm'">LLM（消耗 {{ seedSource.tokens_in }}/{{ seedSource.tokens_out }} tokens）</template>
    <template v-else>手写（不消耗 tokens）</template>
  </div>
</div>
```

- [ ] **Step 12: 删原"满意"按钮相关 markup**

把：

```vue
<Button
  v-if="previewOutput && !previewAccepted"
  kind="primary"
  @click="onAcceptPreview"
>满意,使用此结果</Button>
<Button
  v-else-if="previewAccepted"
  kind="primary"
  @click="onReselectPreview"
>已选 ✓ 重新选</Button>
```

整段删除。

- [ ] **Step 13: 加 CSS（如需要）**

在 `<style scoped>` 区加：

```css
.preview-label {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.label-actions {
  display: flex;
  gap: 4px;
}
.inline-gen-btn {
  font-size: 12px;
  padding: 2px 8px;
}
.seed-output {
  width: 100%;
  font-family: var(--font-serif);
  font-size: 13px;
}
.seed-source-hint {
  font-size: 11px;
  color: var(--text-muted);
  margin-top: 4px;
}
```

- [ ] **Step 14: 编译 + 跑 vite 检查**

```bash
pnpm tsc --noEmit 2>&1 | head -30
```

预期：本文件编译通过。

- [ ] **Step 15: Commit**

```bash
git add src/components/CreateBatchDialog.vue
git commit -m "feat(CreateBatchDialog): 三区 UI + 种子可选化,移除"采用/重新选""
```

---

## Task 8: 前端 vitest — 写 9 个 createBatchDialog 场景

**Files:**
- Modify: `src/__tests__/createBatchDialog.spec.ts`（当前是 placeholder）

- [ ] **Step 1: 替换 placeholder 内容**

把整个文件替换为：

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { setActivePinia, createPinia } from 'pinia';

vi.mock('../ipc/commands', () => ({
  listPrompts: vi.fn(async () => ([
    { id: 1, name: 'style-prompt', kind: 'style', template: '...', is_builtin: false, archived: 0 },
  ])),
  listModels: vi.fn(async () => ([
    { id: 1, name: 'm', base_url: 'http://x', api_key: 'k', model: 'm',
      max_tokens: null, max_context: null, temperature: null,
      disable_thinking: false, concurrency: 1, archived: 0 },
  ])),
  previewFirstChapter: vi.fn(async () => ({
    content: 'LLM 输出内容', tokens_in: 100, tokens_out: 50,
  })),
  getChapter: vi.fn(async () => ({
    id: 100, data_asset_id: 1, idx: 0, title: 'ch0',
    body: '原文正文', word_count: 10, source_kind: 'original',
    source_chapter_id: null, edited_at: null,
  })),
}));

import CreateBatchDialog from '../components/CreateBatchDialog.vue';

beforeEach(() => {
  setActivePinia(createPinia());
  vi.clearAllMocks();
});

const defaultProps = {
  tnId: 1,
  selectedChapterIds: [100],
  previewChapterId: 100,
};

function mountDialog(overrides: Record<string, unknown> = {}) {
  return mount(CreateBatchDialog, {
    props: { ...defaultProps, ...overrides },
    attachTo: document.body,
  });
}

async function fillRequired(dialog: ReturnType<typeof mountDialog>) {
  // 等待 dialog 打开时的 listPrompts/listModels 完成
  await flushPromises();
  await dialog.find('select.prompt-select').setValue(1);
  await dialog.find('select.model-select').setValue(1);
  await dialog.find('input.label-input').setValue('test-batch');
  await flushPromises();
}

describe('CreateBatchDialog: 首章种子可选化 (spec 2026-09-01)', () => {
  it('默认状态:seedContent 为空、previewOutput 为空、seedSource 为 null', async () => {
    const dialog = mountDialog();
    await flushPromises();
    expect((dialog.find('textarea.seed-output').element() as HTMLTextAreaElement).value).toBe('');
    expect((dialog.find('textarea.preview-output').element() as HTMLTextAreaElement).value).toBe('');
  });

  it('提交且 seedContent 为空: payload.preview_first_chapter = null', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    // 不调 previewFirstChapter;不手写
    await dialog.find('button.create-btn').trigger('click');  // 假设创建按钮有 .create-btn
    await flushPromises();
    const emitted = dialog.emitted('submit');
    expect(emitted).toBeTruthy();
    expect(emitted![0][0].preview_first_chapter).toBeNull();
  });

  it('手写后提交: payload.preview_first_chapter.source = { kind: "manual" }', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    await dialog.find('textarea.seed-output').setValue('我手写的内容');
    await flushPromises();
    // 点击"创建"按钮
    await dialog.find('button.create-btn').trigger('click');
    await flushPromises();
    const payload = dialog.emitted('submit')![0][0];
    expect(payload.preview_first_chapter.content).toBe('我手写的内容');
    expect(payload.preview_first_chapter.source).toEqual({ kind: 'manual' });
  });

  it('生成预览 + 复制后提交: payload.preview_first_chapter.source = { kind: "llm", tokens_in, tokens_out }', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    await dialog.find('button.gen-preview-btn').trigger('click');  // 假设生成按钮有 .gen-preview-btn
    await flushPromises();
    await dialog.find('button.copy-btn').trigger('click');  // 假设复制按钮有 .copy-btn
    await flushPromises();
    await dialog.find('button.create-btn').trigger('click');
    await flushPromises();
    const payload = dialog.emitted('submit')![0][0];
    expect(payload.preview_first_charter.source.kind).toBe('llm');
    expect(payload.preview_first_chapter.source.tokens_in).toBe(100);
    expect(payload.preview_first_chapter.source.tokens_out).toBe(50);
  });

  it('切换 previewChapterId: seedContent / previewOutput 被清空', async () => {
    const dialog = mountDialog();
    await flushPromises();
    // 手写一些内容
    await dialog.find('textarea.seed-output').setValue('initial');
    await flushPromises();
    // 切换 props.previewChapterId
    await dialog.setProps({ previewChapterId: 999 });
    await flushPromises();
    expect((dialog.find('textarea.seed-output').element() as HTMLTextAreaElement).value).toBe('');
  });

  it('重选 prompt / model: seedContent 不被清', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    await dialog.find('textarea.seed-output').setValue('user content');
    await flushPromises();
    // 重选 prompt
    await dialog.find('select.prompt-select').setValue(1);  // 同一值,触发 change
    await flushPromises();
    expect((dialog.find('textarea.seed-output').element() as HTMLTextAreaElement).value).toBe('user content');
  });

  it('"↑ 复制"按钮在 previewOutput 为空时禁用', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    const copyBtn = dialog.find('button.copy-btn');
    expect(copyBtn.attributes('disabled')).toBeDefined();
  });

  it('"清空"按钮: seedContent=""、seedSource=null', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const dialog = mountDialog();
    await fillRequired(dialog);
    await dialog.find('textarea.seed-output').setValue('to clear');
    await flushPromises();
    await dialog.find('button.clear-btn').trigger('click');
    await flushPromises();
    expect((dialog.find('textarea.seed-output').element() as HTMLTextAreaElement).value).toBe('');
  });

  it('canSubmit 永真(除基础必填外): seedContent 空也能点创建', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    const createBtn = dialog.find('button.create-btn');
    expect(createBtn.attributes('disabled')).toBeUndefined();
  });
});
```

- [ ] **Step 2: 改 CreateBatchDialog.vue 加测试钩子 class**

为 Task 7 Step 11 的关键按钮加 `class` 属性以便测试：

- 生成预览按钮：加 `class="gen-preview-btn"`
- 从预览复制按钮：加 `class="copy-btn"`
- 清空按钮：加 `class="clear-btn"`
- 创建按钮（footer 内的 primary Button）：加 `class="create-btn"`

- [ ] **Step 3: 跑测试**

```bash
pnpm test src/__tests__/createBatchDialog.spec.ts 2>&1 | tail -30
```

预期：9 个测试全绿。如有 fail，根据 error 信息调整。

- [ ] **Step 4: 跑全 vitest**

```bash
pnpm test 2>&1 | tail -30
```

预期：无回归。

- [ ] **Step 5: Commit**

```bash
git add src/__tests__/createBatchDialog.spec.ts src/components/CreateBatchDialog.vue
git commit -m "test(createBatchDialog): 9 个场景覆盖种子可选化 + 加测试钩子 class"
```

---

## Task 9: 最终验证 + smoke test

**Files:** (无改动)

- [ ] **Step 1: 全 cargo 测试**

```bash
cargo test -p nsc-core 2>&1 | tail -10
```

预期：全绿。

- [ ] **Step 2: 全前端 vitest**

```bash
pnpm test 2>&1 | tail -10
```

预期：全绿。

- [ ] **Step 3: TypeScript 类型检查**

```bash
pnpm tsc --noEmit 2>&1 | tail -10
```

预期：无错误。

- [ ] **Step 4: Rust clippy**

```bash
cargo clippy -p nsc-core --all-targets 2>&1 | tail -10
```

预期：无 warning。

- [ ] **Step 5: 前端 lint（如有）**

```bash
pnpm lint 2>&1 | tail -10
```

预期：无错误。

- [ ] **Step 6: 跑 smoke test 脚本**

```bash
pwsh scripts/smoke.ps1
```

预期：4 秒 GUI 独立测试通过。

- [ ] **Step 7: 手动验证场景（按 spec §10 清单）**

- 打开 dialog → 选章节/prompt/model → 直接点"创建"（不调预览） → 工作流创建成功，首章作为 pending 等候 LLM
- 打开 dialog → 选章节/prompt/model → 点"生成预览" → 点"↑ 复制" → 点"创建" → 工作流创建成功，首章 status=done, tokens 正确
- 打开 dialog → 选章节/prompt/model → 在"转换结果"区手写 → 点"创建" → 工作流创建成功，首章 status=done, tokens_in/out=0
- 打开 dialog → 生成预览 → 不复制 → 直接点"创建" → 与"无预览"同效
- 切换预览章节 → 三区全部清空
- 重选 prompt/model → seedContent 保留
- "↑ 复制"在 previewOutput 为空时禁用
- 重复"↑ 复制"在 seedContent 非空时弹 confirm

逐项勾选。

- [ ] **Step 8: 回归 RegeneratePreviewDialog**

手动打开 `RegeneratePreviewDialog`（在已有工作流的 TransformationNovelDetail 页面 → 选章 → "重新生成"），验证：
- 多 tab 预览正常
- 提交逻辑不变
- `chapter_previews` 表提交后被清空

- [ ] **Step 9: Commit 验证结果（如有 docs 改动）**

如有手测过程中的小修：

```bash
git add -u
git diff --cached --stat
git commit -m "chore: post-verment cleanups"
```

- [ ] **Step 10: 推送并提 PR（如需）**

如本计划在 worktree 中执行，回到主分支后：

```bash
git push origin codex/remove-workflow
gh pr create --title "feat: CreateBatch 首章种子可选化" --body "spec 见 docs/superpowers/specs/2026-09-01-create-batch-first-chapter-seed-design.md"
```

---

## Self-Review Checklist（执行前自己跑）

- [x] **Spec 覆盖**: §3.2 后端类型 → Task 1; §3.3 前端类型 → Task 6; §4.3 canSubmit → Task 7 Step 4; §5.1 三分支 → Task 3 Step 3; §6.1 文件改动 → 8 个 Task 各覆盖一项; §6.3 测试 → Task 5 + Task 8; §7 测试策略 → Task 5 + Task 8; §8 实施步骤 → 8 个 Task 与之 1:1 对应（+ Task 9 验证）
- [x] **占位符**: 无 TBD/TODO/待补充。代码块完整
- [x] **类型一致**: FirstChapterSeed / SeedSource / FirstChapterSeedDto / SeedSourceDto 在 Task 1/4/6 引入，Task 3/4/7 引用一致。字段名 `preview_first_chapter` 在 IPC 边界保留（Task 4/6），`first_chapter_seed` 在 batch_scheduler 内部用（Task 2/3）
- [x] **DRY**: Task 5 用 `fresh_db` + `seed_env` + `seed_batch_with_tcs` 既有 helper;Task 8 用 `flushPromises` + `mountDialog` 既有 pattern
- [x] **TDD**: Task 5 在实现分支后立即写测试（注意:由于 batch_scheduler 已有测试套,本任务以"扩展测试"为主,新增 3 个测试都按"先写测试 → 跑 fail → 实现 → 跑 pass"的精神,本 plan 已合并以减少样板）
- [x] **频繁 commit**: 9 个 commit,每个独立可回滚

## 实施顺序总结

```
Task 1 (models 重命名)
  ↓
Task 2 (WorkflowCreate 字段重命名)
  ↓
Task 3 (apply_preview_in_tx 三分支)
  ↓
Task 4 (commands DTO + DTO)
  ↓
Task 5 (后端测试 4 个)
  ↓
Task 6 (前端 ipc/types)
  ↓
Task 7 (前端 CreateBatchDialog 三区 UI)
  ↓
Task 8 (前端 vitest 9 个场景)
  ↓
Task 9 (最终验证 + 手动测试)
```

每个 Task 可独立 commit + 验证。如某 Task 中途失败,不影响后续 Task 启动。