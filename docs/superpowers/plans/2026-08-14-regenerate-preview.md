# 单章节重新生成预览 — 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在工作流详情 modal 里增加"重新生成预览"对话框,让用户对单个章节不满意时能多次生成 AI 结果对比、编辑、最终提交,不污染工作流列表。

**Architecture:** 新表 `chapter_previews` 持久化预览草稿;中间区域双区(附加指令 + 草稿区);commit 用草稿内容 + 删除所有 preview;AI 走 recorder,business=RegeneratePreview。
**Tech Stack:** Tauri 2 + Vue 3 + Pinia + Rust (rusqlite) + Vite
**前置依赖:** `docs/superpowers/specs/2026-08-14-regenerate-preview-design.md`(已批准)

---

## File Structure

**新增** `migrations/0024_chapter_previews.sql` · `src/components/RegeneratePreviewDialog.vue`

**修改** `crates/nsc-core/src/{db/migrate.rs, models/ai_call_log.rs, models/transformation.rs, db/repo/chapter_preview.rs, db/mod.rs, transformer/batch_scheduler.rs, transformer/transformer.rs, transformer/queue.rs}` · `src-tauri/src/{commands/workflows.rs, lib.rs}` · `src/{ipc/types.ts, ipc/commands.ts, stores/workflows.ts, views/TransformationNovelDetail.vue}`

---

## Task 1: Migration

**Files:** Create `migrations/0024_chapter_previews.sql` · Modify `crates/nsc-core/src/db/migrate.rs`

- [ ] **Step 1.1:** 创建 SQL 文件

写到 `D:\Git\novel-style-converter\migrations\0024_chapter_previews.sql`(UTF-8 无 BOM, CRLF):

```sql
-- Migration 0024: chapter_previews 表(单章节预览草稿)

CREATE TABLE chapter_previews (
  id INTEGER PRIMARY KEY,
  batch_id INTEGER NOT NULL,
  chapter_id INTEGER NOT NULL,
  custom_input TEXT,
  preview_content TEXT,
  tokens_in INTEGER,
  tokens_out INTEGER,
  error TEXT,
  status TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (batch_id) REFERENCES batches(id) ON DELETE CASCADE,
  FOREIGN KEY (chapter_id) REFERENCES chapters(id) ON DELETE CASCADE
);
CREATE INDEX idx_chapter_previews_chap ON chapter_previews(batch_id, chapter_id, id DESC);
```

- [ ] **Step 1.2:** 注册到 SCHEMAS 数组末尾

```rust
    ("0024_chapter_previews", include_str!("../../../../migrations/0024_chapter_previews.sql")),
```

- [ ] **Step 1.3:** 编译 + Commit

```bash
cd D:\Git\novel-style-converter && cargo build -p nsc-core
git add migrations/0024_chapter_previews.sql crates/nsc-core/src/db/migrate.rs
git commit -m "feat(db): migration 0024 chapter_previews table"
```

---

## Task 2: AiCallBusiness + 模型 + Repo

**Files:** Modify `crates/nsc-core/src/models/ai_call_log.rs` · Modify `crates/nsc-core/src/models/transformation.rs` · Create `crates/nsc-core/src/db/repo/chapter_preview.rs` · Modify `crates/nsc-core/src/db/mod.rs`

- [ ] **Step 2.1:** 扩展 AiCallBusiness

在 enum 加 `RegeneratePreview` 变体,在 `as_str` 匹配里追加 `"regenerate_preview"`。

- [ ] **Step 2.2:** 加 PreviewStatus + ChapterPreviewRow 模型

在 `transformation.rs` 末尾加 `PreviewStatus` enum (generating/done/failed) + `FromStr` impl + `ChapterPreviewRow` struct。

- [ ] **Step 2.3:** 创建 ChapterPreviewRepo

新文件 `crates/nsc-core/src/db/repo/chapter_preview.rs`,接口:`insert_generating` / `update_done` / `update_failed` / `list_by_chapter` / `get` / `delete` / `delete_by_chapter`。

- [ ] **Step 2.4:** 导出 repo

在 `db/mod.rs` 加 `pub mod chapter_preview;` + 在 `Db` impl 加 `pub fn chapter_previews(&self) -> ChapterPreviewRepo`。

- [ ] **Step 2.5:** 编译 + Commit

```bash
cargo build -p nsc-core
git add crates/nsc-core/src/models/ai_call_log.rs crates/nsc-core/src/models/transformation.rs crates/nsc-core/src/db/repo/chapter_preview.rs crates/nsc-core/src/db/mod.rs
git commit -m "feat(repo): chapter_previews model + repo"
```

---

## Task 3: BatchScheduler 4 个方法 + AI 调用桥接

**Files:** Modify `crates/nsc-core/src/transformer/{batch_scheduler, transformer, queue}.rs` · Modify `src-tauri/src/lib.rs`

- [ ] **Step 3.1:** 改 `queue.rs::read_context` 为 `pub fn`

- [ ] **Step 3.2:** 给 DefaultTransformer 加参数化方法 `transform_with_business(req, business)`,原 `transform` 改为 wrapper。`Transformer` trait 不动。

- [ ] **Step 3.3:** 给 BatchScheduler 注入 `provider_factory: Arc<dyn Fn(&ModelConfig) -> Box<dyn AiProvider> + Send + Sync>` + `recorder: Arc<dyn AiCallRecorder>`

- [ ] **Step 3.4:** 加 3 个同步方法 `list_chapter_previews` / `discard_preview` / `commit_preview(batch_id, chapter_id, draft_content, source_preview_id)`(单事务写 wrc.content + 更新 tc + 删 chapter_previews)

- [ ] **Step 3.5:** 加 async `regenerate_preview(batch_id, chapter_id, custom_input) -> Result<i64>`:`INSERT` preview 行 → 拼附加指令到 prompt template → `tokio::spawn` 调 `DefaultTransformer::transform_with_business(business=RegeneratePreview)` → 根据 Ok/Err 调 `update_done`/`update_failed`

- [ ] **Step 3.6:** lib.rs 调整:`let provider_factory: ProviderFactory = Arc::new(...)` 提到 let 绑定,传给 `BatchScheduler::new(path, job_queue, provider_factory.clone(), recorder.clone())`

- [ ] **Step 3.7:** 编译 + Commit

```bash
cargo build -p nsc-desktop
git add crates/nsc-core/src/transformer/ src-tauri/src/lib.rs
git commit -m "feat(scheduler): chapter preview 4 methods + parameterized business"
```

---

## Task 4: Tauri commands

**Files:** Modify `src-tauri/src/commands/workflows.rs` · Modify `src-tauri/src/lib.rs`

- [ ] **Step 4.1:** 加 `CommitPreviewInput` DTO(batch_id/chapter_id/draft_content/source_preview_id)

- [ ] **Step 4.2:** 加 4 个 command:`regenerate_chapter_preview` / `commit_chapter_preview` / `list_chapter_previews` / `discard_chapter_preview`

- [ ] **Step 4.3:** 注册到 invoke_handler

- [ ] **Step 4.4:** 编译 + Commit

```bash
cargo build -p nsc-desktop
git add src-tauri/src/commands/workflows.rs src-tauri/src/lib.rs
git commit -m "feat(tauri): chapter preview 4 commands"
```

---

## Task 5: 前端 TS + IPC + Pinia

**Files:** Modify `src/ipc/types.ts` · Modify `src/ipc/commands.ts` · Modify `src/stores/workflows.ts`

- [ ] **Step 5.1:** TS 类型:`PreviewStatus` + `ChapterPreviewRow` + `CommitPreviewInput` + `RegeneratePreviewInput`

- [ ] **Step 5.2:** 4 个 invoke wrapper

- [ ] **Step 5.3:** Pinia store 加 `previewsByChapter: Map<string, ChapterPreviewRow[]>` + `loadPreviews` / `regeneratePreview` / `commitPreview` / `discardPreview`

- [ ] **Step 5.4:** 类型检查 + Commit

```bash
pnpm vue-tsc --noEmit
git add src/ipc/types.ts src/ipc/commands.ts src/stores/workflows.ts
git commit -m "feat(frontend): chapter preview TS + IPC + Pinia"
```

---

## Task 6: RegeneratePreviewDialog

**Files:** Create `src/components/RegeneratePreviewDialog.vue`

Props: `open` / `batchId` / `chapterId` / `chapterIdx` / `chapterTitle`
Emits: `update:open` / `committed`
三栏布局,关键交互(按 spec §6.2):
- 打开:`await store.loadPreviews(batchId, chapterId)`
- [生成]:读附加指令 → `regeneratePreview()` → 轮询 `loadPreviews` 直到 status != generating
- [使用此预览填充草稿]:草稿空 → 直接替换;非空 → 弹追加/替换
- [确认替换](草稿空禁用):confirm() → `commitPreview()` → emit committed
样式用现有 token;preview tab 切换参考 `TransformVersionTabs.vue`。

- [ ] **Step 6.1:** 写组件 + build + Commit

```bash
pnpm build
git add src/components/RegeneratePreviewDialog.vue
git commit -m "feat(ui): RegeneratePreviewDialog 3-column layout"
```

---

## Task 7: 接入 TransformationNovelDetail

**Files:** Modify `src/views/TransformationNovelDetail.vue`

- [ ] **Step 7.1:** cell-actions 拆分:`详情` 链接 + `重新生成` 链接 (status 非 running/pending) + `重试` 链接 (failed/skipped + is_empty_slot)

- [ ] **Step 7.2:** 加 `<RegeneratePreviewDialog>` 接入 + `selectedChapterForRegen` ref + `onPreviewCommitted` handler

- [ ] **Step 7.3:** 删 `reconvertSingle` / `reconvertError`

- [ ] **Step 7.4:** 类型检查 + build + Commit

```bash
pnpm vue-tsc --noEmit && pnpm build
git add src/views/TransformationNovelDetail.vue
git commit -m "feat(tn-detail): cell-actions split regenerate / retry"
```

---

## Task 8: 手动验证

- [ ] **Step 8.1:** `pnpm tauri dev` 启动

- [ ] **Step 8.2:** 核心流程(上传→工作流→跑章节→点重新生成→附加指令→生成→填充→编辑→提交→刷新)

- [ ] **Step 8.3:** 边界(草稿空禁用 / 附加指令超长 / 重开持久化 / [放弃] / 多次生成 / 追加替换)

- [ ] **Step 8.4:** `git log --oneline -10` 检查 commit 顺序

---

## Self-Review

- [x] Spec §1-3 → Task 1-2
- [x] Spec §4 → Task 3.4 / 4.2
- [x] Spec §5 → Task 3-4
- [x] Spec §6 → Task 6-7
- [x] §3.5 → Task 6
- [x] 无 placeholder
- [x] 类型签名一致

## 实施顺序

Task 1 → 2 → 4 → 5 → 6 → 7 → 8。Task 3 最复杂可拆 2 个 commit。