# Prompt 管理 UI 实施计划

> **状态:** 全部 11 个 Task + 4 个实施期修复已落地并合并到 `codex/upload-refactor`(HEAD `b4dbe1e`)。所有 checkbox 已勾选,见每节步骤下方。
> **实施期增补(计划外 commit):**
> - `09665bf fix(ui): escape chapter_content placeholder in warn text`
> - `ad63b08 fix(ui): bind chapter_content placeholder via constant to avoid template parse`
> - `68cc7d0 feat(view): add read-only prompt view dialog`
> - `b4dbe1e fix(view): show builtin prompts via view dialog`
>
> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax (all done).

**Goal:** 在 `/prompts` 路由下提供 prompt CRUD UI,让用户管理 builtin + 自定义 prompts(新建 / 编辑 / 删除 / 复制 builtin),删除前展示引用计数。

**Architecture:**
- 后端新增 `PromptRepo::count_by_prompt`(纯 SQL COUNT)和 5 个 Tauri 命令(`list_prompts / get_prompt / upsert_prompt / delete_prompt / count_transformation_chapters_by_prompt`)。
- 前端新增 Pinia store(`usePromptsStore`)、`Prompts.vue` 列表页 + `PromptEditDialog.vue` 编辑对话框。
- 复用现有 `Dialog.vue` / `Button.vue` / `Input.vue` / `Table.vue` / `Tag.vue`,不引入新基础组件。
- 组件树 / E2E 不写测试(沿用项目惯例,vitest + mock invoke 覆盖 IPC 形状)。

**Tech Stack:** Vue 3.5 + Pinia 2.3 + vue-router 4.6 + TypeScript 5.6(前端),Tauri 2 + rusqlite + nsc-core(Rust 后端)。

---

## 文件结构

**New files:**
- `src-tauri/src/commands/prompts.rs` — 5 个 Tauri 命令
- `src/stores/prompts.ts` — `usePromptsStore` Pinia store
- `src/views/Prompts.vue` — `/prompts` 路由视图(列表 + 表格 + 新建按钮 + 删除 confirm)
- `src/components/PromptEditDialog.vue` — 新建 / 编辑 / 复制 builtin 共用 dialog
- `src/__tests__/prompts.spec.ts` — IPC wrapper + store 测试

**Modified files:**
- `crates/nsc-core/src/db/repo/prompt.rs` — 加 `count_by_prompt(prompt_id) -> Result<i64>`
- `crates/nsc-core/tests/db_prompt.rs` — 加 2 个测试
- `src-tauri/src/commands/mod.rs` — 加 `pub mod prompts;`
- `src-tauri/src/lib.rs` — 注册 5 个命令
- `src/ipc/types.ts` — 加 `Prompt` / `PromptInput` 类型
- `src/ipc/commands.ts` — 加 5 个 wrapper
- `src/router/index.ts` — 加 `/prompts` 路由
- `src/components/Sidebar.vue` — 加 1 项 nav(顺序:transformations 后、models 前)+ SVG icon switch 分支

---

## Task 1: 后端 — 添加 `PromptRepo::count_by_prompt`(TDD)

**Files:**
- Modify: `crates/nsc-core/src/db/repo/prompt.rs`(在 `delete` 方法后面加)
- Modify: `crates/nsc-core/tests/db_prompt.rs`(追加 2 个测试 + 1 个 helper)

- [x] **Step 1: 写失败的测试 + helper**

替换 `crates/nsc-core/tests/db_prompt.rs` 全文为:

```rust
use nsc_core::db::Db;
use nsc_core::models::{
    NewChapter, NewDataAsset, NewTransformationChapter, NewTransformationNovel, NewUpload,
    Prompt, PromptKind, TransformMode,
};
use nsc_core::prompts;

#[test]
fn seed_inserts_only_when_empty() {
    let db = Db::open_in_memory().unwrap();

    db.seed_builtin_prompts().unwrap();
    let first = db.prompts().list().unwrap();
    assert_eq!(first.len(), prompts::builtin_prompts().len());
    assert!(first.iter().all(|p| p.is_builtin));

    db.seed_builtin_prompts().unwrap();
    let second = db.prompts().list().unwrap();
    assert_eq!(second.len(), first.len());
}

/// 准备 1 个 data_asset + 1 个 transformation_novel + 1 个 chapter,
/// 返回 (tn_id, chapter_id)。供 count_by_prompt 测试用。
fn setup_tn(db: &Db) -> (i64, i64) {
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(),
        filename: "x.txt".into(),
        byte_size: 0,
        file_path: "/tmp/x.txt".into(),
        original_text: "正文".into(),
        word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset {
        upload_id,
        title: "DA".into(),
    }).unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id,
        title: "N".into(),
    }).unwrap();
    let cid = db.chapters().insert(&NewChapter {
        data_asset_id: da_id,
        idx: 1,
        title: "Ch 1".into(),
        byte_start: 0,
        byte_end: 6,
        word_count: 2,
    }).unwrap();
    (tn_id, cid)
}

#[test]
fn count_by_prompt_returns_ref_count() {
    let db = Db::open_in_memory().unwrap();
    db.seed_builtin_prompts().unwrap();
    let prompt_a = db.prompts().list().unwrap()[0].id;
    let prompt_b = db.prompts().insert(&Prompt {
        id: 0,
        name: "user".into(),
        kind: PromptKind::Compress,
        template: "x".into(),
        is_builtin: false,
    }).unwrap();

    let (tn_id, cid) = setup_tn(&db);
    for _ in 0..3 {
        db.transformation_chapters().insert(&NewTransformationChapter {
            transformation_novel_id: tn_id,
            chapter_id: cid,
            mode: TransformMode::Compress,
            prompt_id: prompt_a,
            model_config_id: 1,
            ctx_prev_original: 0,
            ctx_prev_transformed: 0,
            ctx_next_original: 0,
        }).unwrap();
    }
    for _ in 0..2 {
        db.transformation_chapters().insert(&NewTransformationChapter {
            transformation_novel_id: tn_id,
            chapter_id: cid,
            mode: TransformMode::Style,
            prompt_id: prompt_b,
            model_config_id: 1,
            ctx_prev_original: 0,
            ctx_prev_transformed: 0,
            ctx_next_original: 0,
        }).unwrap();
    }

    assert_eq!(db.prompts().count_by_prompt(prompt_a).unwrap(), 3);
    assert_eq!(db.prompts().count_by_prompt(prompt_b).unwrap(), 2);
}

#[test]
fn count_by_prompt_zero_for_unused() {
    let db = Db::open_in_memory().unwrap();
    db.seed_builtin_prompts().unwrap();
    let pid = db.prompts().list().unwrap()[0].id;
    assert_eq!(db.prompts().count_by_prompt(pid).unwrap(), 0);
}
```

- [x] **Step 2: 跑测试,确认失败**

Run: `cargo test -p nsc-core --test db_prompt`
Expected: `count_by_prompt_returns_ref_count` 和 `count_by_prompt_zero_for_unused` 编译失败,error 信息含 `no method named count_by_prompt`。

- [x] **Step 3: 实现 `PromptRepo::count_by_prompt`**

在 `crates/nsc-core/src/db/repo/prompt.rs` 的 `delete` 方法(第 52-55 行)后面追加:

```rust
    /// 统计 `transformation_chapters` 表里 prompt_id 等于参数的行数。
    /// 删除 prompt 前给用户展示"被 N 个转换结果引用"用。
    pub fn count_by_prompt(&self, prompt_id: i64) -> Result<i64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM transformation_chapters WHERE prompt_id = ?1",
            params![prompt_id],
            |r| r.get(0),
        )?;
        Ok(n)
    }
```

- [x] **Step 4: 跑测试,确认通过**

Run: `cargo test -p nsc-core --test db_prompt`
Expected: 3 个测试全过(`seed_inserts_only_when_empty` + `count_by_prompt_returns_ref_count` + `count_by_prompt_zero_for_unused`)。

- [x] **Step 5: 提交**

```bash
git add crates/nsc-core/src/db/repo/prompt.rs crates/nsc-core/tests/db_prompt.rs
git commit -m "feat(prompts): PromptRepo::count_by_prompt returns ref count"
```

---

## Task 2: 后端 — 创建 `src-tauri/src/commands/prompts.rs`(5 个 Tauri 命令)

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`(加 `pub mod prompts;`)
- Create: `src-tauri/src/commands/prompts.rs`

- [x] **Step 1: 在 `src-tauri/src/commands/mod.rs` 末尾追加一行**

在 `src-tauri/src/commands/mod.rs` 现有 `pub mod transformations;` 那一行后追加:

```rust
pub mod prompts;
```

最终文件应该是:

```rust
pub mod chapters;
pub mod cleaning;
pub mod data_assets;
pub mod models;
pub mod prompts;
pub mod transformation_novels;
pub mod transformations;
pub mod uploads;
```

(按字母顺序排列;或者就追加在末尾,保持原有顺序不动 — 两种都可以,选与现有风格一致的那个。现有文件看起来是按 `chapters / cleaning / data_assets / models / transformation_novels / transformations / uploads` 排列,不是严格字母序。把 `prompts` 插在 `models` 之后,跟字母序最接近。)

- [x] **Step 2: 创建 `src-tauri/src/commands/prompts.rs`**

文件内容:

```rust
use std::sync::{Arc, Mutex};

use nsc_core::db::Db;
use nsc_core::models::{Prompt, PromptKind};
use serde::Deserialize;
use tauri::State;

#[tauri::command]
pub fn list_prompts(db: State<'_, Arc<Mutex<Db>>>) -> Result<Vec<Prompt>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.prompts().list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_prompt(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<Prompt, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.prompts()
        .get(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("prompt {id} 不存在"))
}

/// `upsert_prompt` 入参。`id == 0` 走 insert(返回新 id);>0 走 update(返回传入 id)。
/// 内层 DTO 没有 `#[serde(rename_all = "snake_case")]`(字段全单词),前端按字段名原样发。
/// `kind` 用 `PromptKind`,后端 `#[serde(rename_all = "snake_case")]` 自动映射 `"compress"` / `"style"`。
#[derive(Debug, Deserialize)]
pub struct PromptInput {
    pub id: i64,
    pub name: String,
    pub kind: PromptKind,
    pub template: String,
}

#[tauri::command]
pub fn upsert_prompt(
    db: State<'_, Arc<Mutex<Db>>>,
    payload: PromptInput,
) -> Result<i64, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    if payload.id == 0 {
        let new = Prompt {
            id: 0,
            name: payload.name,
            kind: payload.kind,
            template: payload.template,
            is_builtin: false,
        };
        db.prompts().insert(&new).map_err(|e| e.to_string())
    } else {
        // 更新前先读现有的 is_builtin:UI 不会让 builtin 进 update 流程,
        // 但万一收到 builtin 的 update,也保留 builtin 标记,不静默改写。
        let existing = db
            .prompts()
            .get(payload.id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("prompt {} 不存在", payload.id))?;
        let updated = Prompt {
            id: existing.id,
            name: payload.name,
            kind: payload.kind,
            template: payload.template,
            is_builtin: existing.is_builtin,
        };
        db.prompts().update(&updated).map_err(|e| e.to_string())?;
        Ok(updated.id)
    }
}

#[tauri::command]
pub fn delete_prompt(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.prompts().delete(id).map_err(|e| e.to_string())
}

/// 统计 prompt 被 `transformation_chapters` 引用的次数。
/// 前端删除 prompt 前展示"被 N 个转换结果引用",N=0 不展示。
#[tauri::command]
pub fn count_transformation_chapters_by_prompt(
    db: State<'_, Arc<Mutex<Db>>>,
    prompt_id: i64,
) -> Result<i64, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.prompts()
        .count_by_prompt(prompt_id)
        .map_err(|e| e.to_string())
}
```

- [x] **Step 3: 编译验证**

Run: `cargo build --workspace`
Expected: 成功,无 error。如果 `crates/nsc-desktop` 是空 legacy,`cargo build --workspace` 可能跳过他(由 workspace 配置决定),无需处理。

- [x] **Step 4: 提交**

```bash
git add src-tauri/src/commands/prompts.rs src-tauri/src/commands/mod.rs
git commit -m "feat(tauri): commands for prompts CRUD + ref count"
```

---

## Task 3: 后端 — 在 `lib.rs` 注册 5 个 Tauri 命令

**Files:**
- Modify: `src-tauri/src/lib.rs:47-80`(在 `invoke_handler!` 列表里加 5 行)

- [x] **Step 1: 在 `invoke_handler!` 加 5 行**

在 `commands::transformations::get_queue_snapshot,` 那一行(目前在列表末尾)之后追加 5 行:

```rust
            commands::prompts::list_prompts,
            commands::prompts::get_prompt,
            commands::prompts::upsert_prompt,
            commands::prompts::delete_prompt,
            commands::prompts::count_transformation_chapters_by_prompt,
```

(注意缩进 12 空格,跟现有行一致)

- [x] **Step 2: 编译验证**

Run: `cargo build --workspace`
Expected: 成功,无 error。如果有"command not in allowlist"或"unused"警告也忽略。

- [x] **Step 3: 跑全部后端测试**

Run: `cargo test -p nsc-core`
Expected: 全部通过,包含 Task 1 新增的 `count_by_prompt_*` 测试。

- [x] **Step 4: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "chore(tauri): register prompts commands in invoke_handler"
```

---

## Task 4: 前端 — 在 `src/ipc/types.ts` 加 `Prompt` / `PromptInput`

**Files:**
- Modify: `src/ipc/types.ts`(在文件末尾追加)

- [x] **Step 1: 在 `types.ts` 末尾追加 2 个类型**

定位:文件末尾,`QueueSnapshot` interface 之后。直接追加:

```ts
/**
 * 后端 `prompts` 表行的前端镜像(取自 `nsc_core::models::Prompt`)。
 * `kind` 来自后端 `PromptKind` 枚举(`#[serde(rename_all = "snake_case")]`)
 * —— 前端拿到 / 发回 `"compress"` / `"style"`。
 * `is_builtin` 为 true 的行在 UI 上不可编辑、不可删除,可"复制"成用户版。
 */
export interface Prompt {
  id: number;
  name: string;
  kind: 'compress' | 'style';
  template: string;
  is_builtin: boolean;
}

/**
 * `upsert_prompt` 入参。`id === 0` 表示新建(走 insert);>0 表示更新(走 update)。
 * 字段保持 snake_case-by-default —— `kind` / `name` / `template` 都是单词,
 * 没有 `#[serde(rename_all)]` 在这层 DTO 上,所以前端按字段名原样发。
 */
export type PromptInput = Omit<Prompt, 'id' | 'is_builtin'> & { id: number };
```

- [x] **Step 2: 类型检查**

Run: `pnpm exec tsc --noEmit`
Expected: 无 error。

- [x] **Step 3: 提交**

```bash
git add src/ipc/types.ts
git commit -m "feat(ipc): add Prompt / PromptInput types"
```

---

## Task 5: 前端 — 写 `src/__tests__/prompts.spec.ts` + 加 5 个 IPC wrapper(TDD)

**Files:**
- Create: `src/__tests__/prompts.spec.ts`
- Modify: `src/ipc/commands.ts`(加 5 个 wrapper)

- [x] **Step 1: 创建 `src/__tests__/prompts.spec.ts`,只含 wrapper 测试(失败版)**

文件内容:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import {
  listPrompts,
  getPrompt,
  upsertPrompt,
  deletePrompt,
  countPromptUsage,
} from '../ipc/commands';
import type { Prompt, PromptInput } from '../ipc/types';

describe('prompts IPC wrappers', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('listPrompts calls list_prompts', async () => {
    const payload = [
      { id: 1, name: 'compress_default', kind: 'compress', template: '...', is_builtin: true },
    ] as Prompt[];
    vi.mocked(invoke).mockResolvedValueOnce(payload);

    const result = await listPrompts();

    expect(invoke).toHaveBeenCalledWith('list_prompts');
    expect(result).toEqual(payload);
  });

  it('getPrompt calls get_prompt with id', async () => {
    const p = { id: 7, name: 'x', kind: 'style', template: '...', is_builtin: false } as Prompt;
    vi.mocked(invoke).mockResolvedValueOnce(p);

    const result = await getPrompt(7);

    expect(invoke).toHaveBeenCalledWith('get_prompt', { id: 7 });
    expect(result).toBe(p);
  });

  it('upsertPrompt calls upsert_prompt with payload wrapper', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(11);

    const input: PromptInput = {
      id: 0,
      name: 'new',
      kind: 'compress',
      template: 'hello',
    };
    const id = await upsertPrompt(input);

    expect(invoke).toHaveBeenCalledWith('upsert_prompt', {
      payload: { id: 0, name: 'new', kind: 'compress', template: 'hello' },
    });
    expect(id).toBe(11);
  });

  it('deletePrompt calls delete_prompt with id', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await deletePrompt(7);

    expect(invoke).toHaveBeenCalledWith('delete_prompt', { id: 7 });
  });

  it('countPromptUsage calls count_transformation_chapters_by_prompt with promptId', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(3);

    const n = await countPromptUsage(7);

    expect(invoke).toHaveBeenCalledWith('count_transformation_chapters_by_prompt', { promptId: 7 });
    expect(n).toBe(3);
  });
});
```

- [x] **Step 2: 跑测试,确认失败**

Run: `pnpm test -- prompts`
Expected: 5 个 `prompts IPC wrappers` 全 fail,error 信息含 "not a function" 或 "Cannot find module" —— 因为 `commands.ts` 还没导出这些函数。

- [x] **Step 3: 在 `src/ipc/commands.ts` 加 5 个 wrapper + 导入 Prompt / PromptInput 类型**

修改 1:文件顶部 import block 改为:

```ts
import type {
  ModelConfig, ModelConfigInput,
  UploadSummary, CleaningPreview,
  DataAssetChapter, DataAssetRow, CommitDataAssetInput,
  ChapterSegment, ChapterMeta, ChapterContentRow, Chapter, ChapterInput,
  TransformationNovelSummary, TransformationChapterRow,
  EnqueuePayload, EnqueueAllPayload, QueueSnapshot,
  Prompt, PromptInput,
} from './types';
```

修改 2:在文件末尾(Queue 段后)追加新段:

```ts
// ─── Prompts ───────────────────────────────────────────────────────────────
export function listPrompts(): Promise<Prompt[]> {
  return invoke<Prompt[]>('list_prompts');
}

export function getPrompt(id: number): Promise<Prompt> {
  return invoke<Prompt>('get_prompt', { id });
}

export function upsertPrompt(payload: PromptInput): Promise<number> {
  return invoke<number>('upsert_prompt', { payload });
}

export function deletePrompt(id: number): Promise<void> {
  return invoke<void>('delete_prompt', { id });
}

export function countPromptUsage(promptId: number): Promise<number> {
  return invoke<number>('count_transformation_chapters_by_prompt', { promptId });
}
```

- [x] **Step 4: 跑测试,确认通过**

Run: `pnpm test -- prompts`
Expected: 5 个 wrapper 测试全过。

- [x] **Step 5: 提交**

```bash
git add src/ipc/commands.ts src/__tests__/prompts.spec.ts
git commit -m "feat(ipc): add 5 prompt wrappers + tests"
```

---

## Task 6: 前端 — 写 `src/stores/prompts.ts` Pinia store(TDD)

**Files:**
- Create: `src/stores/prompts.ts`
- Modify: `src/__tests__/prompts.spec.ts`(追加 store 测试)

- [x] **Step 1: 在 `prompts.spec.ts` 追加 store 测试(失败版)**

在 `src/__tests__/prompts.spec.ts` 文件末尾追加:

```ts
import { setActivePinia, createPinia } from 'pinia';
import { usePromptsStore } from '../stores/prompts';

describe('prompts store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(invoke).mockReset();
  });

  it('load calls list_prompts and stores result', async () => {
    const data = [
      { id: 1, name: 'compress_default', kind: 'compress', template: '...', is_builtin: true },
      { id: 2, name: 'style_default', kind: 'style', template: '...', is_builtin: true },
      { id: 3, name: 'user', kind: 'compress', template: '...', is_builtin: false },
    ] as Prompt[];
    vi.mocked(invoke).mockResolvedValueOnce(data);

    const store = usePromptsStore();
    await store.load();

    expect(invoke).toHaveBeenCalledWith('list_prompts');
    expect(store.prompts).toHaveLength(3);
    expect(store.loading).toBe(false);
  });

  it('upsert (id=0, create) invokes upsert_prompt then reloads', async () => {
    const store = usePromptsStore();
    vi.mocked(invoke).mockResolvedValueOnce(11); // upsert
    vi.mocked(invoke).mockResolvedValueOnce([]); // load
    await store.upsert({ id: 0, name: 'new', kind: 'compress', template: 't' });

    expect(invoke).toHaveBeenNthCalledWith(1, 'upsert_prompt', {
      payload: { id: 0, name: 'new', kind: 'compress', template: 't' },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'list_prompts');
  });

  it('upsert (id>0, update) invokes upsert_prompt then reloads', async () => {
    const store = usePromptsStore();
    vi.mocked(invoke).mockResolvedValueOnce(11); // upsert
    vi.mocked(invoke).mockResolvedValueOnce([]); // load
    await store.upsert({ id: 7, name: 'edit', kind: 'style', template: 't' });

    expect(invoke).toHaveBeenNthCalledWith(1, 'upsert_prompt', {
      payload: { id: 7, name: 'edit', kind: 'style', template: 't' },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'list_prompts');
  });

  it('remove invokes delete_prompt then reloads', async () => {
    const store = usePromptsStore();
    vi.mocked(invoke).mockResolvedValueOnce(undefined); // delete
    vi.mocked(invoke).mockResolvedValueOnce([]); // load
    await store.remove(7);

    expect(invoke).toHaveBeenNthCalledWith(1, 'delete_prompt', { id: 7 });
    expect(invoke).toHaveBeenNthCalledWith(2, 'list_prompts');
  });

  it('countUsage returns number from invoke', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(5);
    const store = usePromptsStore();
    const n = await store.countUsage(7);
    expect(invoke).toHaveBeenCalledWith('count_transformation_chapters_by_prompt', { promptId: 7 });
    expect(n).toBe(5);
  });

  it('captures error string on load failure', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error('boom'));
    const store = usePromptsStore();
    await store.load();
    expect(store.error).toBe('boom');
  });
});
```

- [x] **Step 2: 跑测试,确认失败(store 部分)**

Run: `pnpm test -- prompts`
Expected: 6 个 store 测试全 fail,error 含 "Cannot find module '../stores/prompts'" 或 usePromptsStore 未导出。前 5 个 wrapper 测试仍 pass。

- [x] **Step 3: 创建 `src/stores/prompts.ts`**

文件内容:

```ts
import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { Prompt, PromptInput } from '../ipc/types';
import {
  countPromptUsage as ipcCountPromptUsage,
  deletePrompt as ipcDeletePrompt,
  listPrompts as ipcListPrompts,
  upsertPrompt as ipcUpsertPrompt,
} from '../ipc/commands';

/// 写后 reload 比 diff 简单,prompt 表典型 < 20 条,成本可忽略。
export const usePromptsStore = defineStore('prompts', () => {
  const prompts = ref<Prompt[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function load() {
    loading.value = true;
    error.value = null;
    try {
      prompts.value = await ipcListPrompts();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  async function upsert(payload: PromptInput): Promise<number> {
    const id = await ipcUpsertPrompt(payload);
    await load();
    return id;
  }

  async function remove(id: number): Promise<void> {
    await ipcDeletePrompt(id);
    await load();
  }

  /// 引用计数 —— 删除 prompt 前弹 confirm 用。返回 Promise<number>,让调用方 await。
  async function countUsage(id: number): Promise<number> {
    return ipcCountPromptUsage(id);
  }

  return { prompts, loading, error, load, upsert, remove, countUsage };
});
```

- [x] **Step 4: 跑测试,确认通过**

Run: `pnpm test -- prompts`
Expected: 11 个测试全过(5 wrapper + 6 store)。

- [x] **Step 5: 提交**

```bash
git add src/stores/prompts.ts src/__tests__/prompts.spec.ts
git commit -m "feat(store): usePromptsStore with load/upsert/remove/countUsage"
```

---

## Task 7: 前端 — 创建 `src/components/PromptEditDialog.vue`

**Files:**
- Create: `src/components/PromptEditDialog.vue`

> 组件树不写单测,沿用项目惯例(详见 spec §8.2)。本任务靠类型检查 + 后续 dev server 手动验证。

- [x] **Step 1: 创建文件**

文件内容:

```vue
<template>
  <Dialog v-model:open="open" :title="title" :width="560">
    <div class="row">
      <label>名称 *</label>
      <Input v-model="nameRef" :placeholder="namePlaceholder" />
    </div>
    <div class="row">
      <label>kind *</label>
      <select v-model="kindRef" class="kind-select">
        <option value="compress">压缩</option>
        <option value="style">文风</option>
      </select>
    </div>
    <div class="row column">
      <label>template *</label>
      <textarea
        v-model="templateRef"
        class="template-area"
        rows="14"
        spellcheck="false"
      />
    </div>
    <div v-if="missingChapterContent" class="warn">
      该 prompt 未引用 <code>{{ '{{chapter_content}}' }}</code>,LLM 将无法看到章节正文
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <template #footer>
      <Button @click="open = false">取消</Button>
      <Button kind="primary" :disabled="!canSubmit" :loading="submitting" @click="onSubmit">
        保存
      </Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import Input from './ui/Input.vue';
import type { Prompt, PromptInput } from '../ipc/types';
import { usePromptsStore } from '../stores/prompts';

const props = defineProps<{
  mode: 'create' | 'edit' | 'copy-from-builtin';
  initial?: Prompt;
}>();

const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{ saved: [] }>();

const store = usePromptsStore();
const nameRef = ref('');
const kindRef = ref<'compress' | 'style'>('compress');
const templateRef = ref('');
const submitting = ref(false);
const error = ref<string | null>(null);

const title = computed(() => ({
  create: '新建 prompt',
  edit: '编辑 prompt',
  'copy-from-builtin': '复制 builtin prompt',
}[props.mode]));

const namePlaceholder = computed(() => {
  if (props.mode === 'copy-from-builtin') return '原 builtin 名称 _copy';
  return '例如:compress_v2';
});

const canSubmit = computed(
  () =>
    nameRef.value.trim() !== '' &&
    templateRef.value.trim() !== '' &&
    !submitting.value,
);

const missingChapterContent = computed(
  () => !templateRef.value.includes('{{chapter_content}}'),
);

function blank() {
  nameRef.value = '';
  kindRef.value = 'compress';
  templateRef.value = '';
  error.value = null;
  submitting.value = false;
}

function applyInitial(value: Prompt | undefined) {
  blank();
  if (!value) return;
  nameRef.value = value.name;
  kindRef.value = value.kind;
  templateRef.value = value.template;
  if (props.mode === 'copy-from-builtin' && !value.name.endsWith('_copy')) {
    nameRef.value = `${value.name}_copy`;
  }
}

watch(() => props.initial, (v) => applyInitial(v), { immediate: true });
watch(open, (v) => {
  if (v) applyInitial(props.initial);
});

async function onSubmit() {
  if (!canSubmit.value) return;
  submitting.value = true;
  error.value = null;
  try {
    const payload: PromptInput = {
      id: props.mode === 'create' ? 0 : (props.initial?.id ?? 0),
      name: nameRef.value.trim(),
      kind: kindRef.value,
      template: templateRef.value,
    };
    await store.upsert(payload);
    emit('saved');
    open.value = false;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    submitting.value = false;
  }
}
</script>

<style scoped>
.row {
  display: flex;
  align-items: center;
  margin-bottom: 12px;
  gap: 12px;
}
.row.column {
  flex-direction: column;
  align-items: stretch;
}
.row label {
  width: 100px;
  font-size: 14px;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.kind-select {
  flex: 1;
  height: 32px;
  padding: 0 8px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
  color: var(--text-primary);
  font-size: 14px;
  font-family: inherit;
  outline: none;
}
.kind-select:focus { border-color: var(--border-strong); }
.template-area {
  width: 100%;
  padding: 10px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
  color: var(--text-primary);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 13px;
  line-height: 1.5;
  resize: vertical;
  outline: none;
  box-sizing: border-box;
}
.template-area:focus { border-color: var(--border-strong); }
.warn {
  margin-top: 8px;
  padding: 8px 12px;
  background: #fff8e1;
  color: #8a6d3b;
  border-radius: var(--radius-pin);
  font-size: 12px;
}
.warn code {
  background: rgba(0, 0, 0, 0.05);
  padding: 1px 4px;
  border-radius: 3px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}
.error {
  margin-top: 8px;
  color: var(--color-cinnabar-deep);
  font-size: 12px;
}
</style>
```

- [x] **Step 2: 类型检查**

Run: `pnpm exec tsc --noEmit`
Expected: 无 error。

- [x] **Step 3: 提交**

```bash
git add src/components/PromptEditDialog.vue
git commit -m "feat(ui): PromptEditDialog for create/edit/copy"
```

---

## Task 8: 前端 — 创建 `src/views/Prompts.vue`

**Files:**
- Create: `src/views/Prompts.vue`

> 组件不写单测,靠手动 dev server 验证。

- [x] **Step 1: 创建文件**

文件内容:

```vue
<template>
  <section>
    <header class="header">
      <h2>提示词</h2>
      <div class="actions">
        <Button kind="primary" @click="openCreate">新建 prompt</Button>
      </div>
    </header>

    <div v-if="store.error" class="alert">{{ store.error }}</div>

    <div v-if="!store.loading && store.prompts.length === 0" class="empty">
      还没有 prompt(实际不会触发,seed 后永远有 2 个 builtin)。
    </div>

    <Table
      v-else
      :columns="columns"
      :data="store.prompts"
      :row-key="(row) => row.id"
    >
      <template #cell-name="{ row }">{{ row.name }}</template>
      <template #cell-kind="{ row }">
        <span class="kind-tag" :class="`kind-${row.kind}`">
          {{ row.kind === 'compress' ? '压缩' : '文风' }}
        </span>
      </template>
      <template #cell-builtin="{ row }">
        <Tag v-if="row.is_builtin" kind="info">内置</Tag>
        <span v-else class="muted">用户</span>
      </template>
      <template #cell-actions="{ row }">
        <Button
          size="small"
          :disabled="row.is_builtin"
          @click="openEdit(row)"
        >编辑</Button>
        <Button size="small" @click="openCopy(row)">复制</Button>
        <Button
          size="small"
          kind="danger"
          :disabled="row.is_builtin"
          @click="onDelete(row.id, row.name)"
        >删除</Button>
      </template>
    </Table>

    <PromptEditDialog
      v-model:open="dialogOpen"
      :mode="dialogMode"
      :initial="dialogInitial"
      @saved="onSaved"
    />

    <Dialog v-model:open="confirmOpen" title="删除 prompt" :width="420">
      <p v-if="pendingDelete">
        确认删除 prompt "<strong>{{ pendingDelete.name }}</strong>"?
        <span v-if="pendingDelete.usage > 0" class="usage">
          该 prompt 被 <strong>{{ pendingDelete.usage }}</strong> 个转换结果引用,
          删除后这些引用将变成孤儿(history 行 prompt_id 仍保留,UI 按 id 读)。
        </span>
      </p>
      <template #footer>
        <Button @click="confirmOpen = false">取消</Button>
        <Button kind="danger" @click="confirmDelete">确认删除</Button>
      </template>
    </Dialog>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue';
import Button from '../components/ui/Button.vue';
import Dialog from '../components/ui/Dialog.vue';
import Table from '../components/ui/Table.vue';
import Tag from '../components/ui/Tag.vue';
import PromptEditDialog from '../components/PromptEditDialog.vue';
import { usePromptsStore } from '../stores/prompts';
import type { Prompt } from '../ipc/types';

const store = usePromptsStore();

const columns = [
  { key: 'name', title: '名称', width: '220px' },
  { key: 'kind', title: '类型', width: '100px' },
  { key: 'builtin', title: '来源', width: '100px' },
  { key: 'actions', title: '操作', width: '280px' },
];

type DialogMode = 'create' | 'edit' | 'copy-from-builtin';
const dialogOpen = ref(false);
const dialogMode = ref<DialogMode>('create');
const dialogInitial = ref<Prompt | undefined>(undefined);

interface PendingDelete { id: number; name: string; usage: number }
const confirmOpen = ref(false);
const pendingDelete = ref<PendingDelete | null>(null);

onMounted(() => store.load());

function openCreate() {
  dialogMode.value = 'create';
  dialogInitial.value = undefined;
  dialogOpen.value = true;
}

function openEdit(row: Prompt) {
  dialogMode.value = 'edit';
  dialogInitial.value = row;
  dialogOpen.value = true;
}

function openCopy(row: Prompt) {
  dialogMode.value = 'copy-from-builtin';
  dialogInitial.value = row;
  dialogOpen.value = true;
}

function onSaved() {
  // store.upsert 内部已 reload,无需再 load
}

async function onDelete(id: number, name: string) {
  // 引用计数失败不阻塞删除流程 —— confirm 里不展示 usage,弹 N=0 即可
  let usage = 0;
  try {
    usage = await store.countUsage(id);
  } catch {
    usage = 0;
  }
  pendingDelete.value = { id, name, usage };
  confirmOpen.value = true;
}

async function confirmDelete() {
  const target = pendingDelete.value;
  if (!target) return;
  confirmOpen.value = false;
  pendingDelete.value = null;
  try {
    await store.remove(target.id);
  } catch (e: unknown) {
    alert(e instanceof Error ? e.message : String(e));
  }
}
</script>

<style scoped>
.header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  margin-bottom: 24px;
  padding-bottom: 16px;
  border-bottom: 1px solid var(--border-color);
}
.header h2 { margin: 0; }
.actions { display: flex; gap: 12px; align-items: center; }
.alert {
  padding: 12px 16px;
  background: var(--bg-hover);
  color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin);
  margin-bottom: 12px;
}
.empty {
  text-align: center;
  padding: 56px 0;
  color: var(--text-secondary);
  border: 1px dashed var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
}
.muted { color: var(--text-secondary); font-size: 12px; }
.kind-tag {
  display: inline-block;
  padding: 2px 10px;
  border-radius: var(--radius-pin);
  font-size: 12px;
}
.kind-compress { background: var(--color-paper-mist); color: var(--text-primary); }
.kind-style { background: var(--color-cinnabar-light); color: var(--color-cinnabar-deep); }
.usage {
  display: block;
  margin-top: 8px;
  font-size: 13px;
  color: var(--text-secondary);
}
</style>
```

- [x] **Step 2: 类型检查**

Run: `pnpm exec tsc --noEmit`
Expected: 无 error。

- [x] **Step 3: 提交**

```bash
git add src/views/Prompts.vue
git commit -m "feat(view): Prompts.vue list + create/edit/copy/delete"
```

---

## Task 9: 前端 — 在 `src/router/index.ts` 加 `/prompts` 路由

**Files:**
- Modify: `src/router/index.ts`(加 import + 1 行 route)

- [x] **Step 1: 在 import 段加 `Prompts` 导入**

把文件顶部的 import block 改为:

```ts
import { createRouter, createWebHistory } from 'vue-router';
import Models from '../views/Models.vue';
import Library from '../views/Library.vue';
import Upload from '../views/Upload.vue';
import DataAsset from '../views/DataAsset.vue';
import ParseWizard from '../views/parse.vue';
import Transform from '../views/Transform.vue';
import Prompts from '../views/Prompts.vue';
import { findDataAssetByUpload } from '../ipc/commands';
```

- [x] **Step 2: 在 routes 数组加 1 行**

在 `{ path: '/models', component: Models },` 之前(也就是 `/models` 路由前)加:

```ts
    { path: '/prompts', component: Prompts },
```

最终 routes 数组末尾应该是:

```ts
    { path: '/library/transform/:chapterId', component: Transform, name: 'transform' },
    { path: '/prompts', component: Prompts },
    { path: '/models', component: Models },
  ],
```

- [x] **Step 3: 类型检查 + dev server 启 5s 验通**

Run:
```bash
pnpm exec tsc --noEmit
```
Expected: 无 error。

Run(后台,5s 后停):
```bash
pnpm dev &
sleep 5
kill %1 2>/dev/null
```
Expected: Vite 启动,无路由相关报错。

- [x] **Step 4: 提交**

```bash
git add src/router/index.ts
git commit -m "feat(router): add /prompts route"
```

---

## Task 10: 前端 — 在 `src/components/Sidebar.vue` 加 prompts nav 项 + icon

**Files:**
- Modify: `src/components/Sidebar.vue`(icon switch + Item 类型 + topItems 数组)

- [x] **Step 1: 加 icon switch 分支**

定位:Sidebar.vue 的 `<template>` 里 SVG `<template v-else>` 那一行(第 28 行附近,是 model icon 的 fallback)。在它前面加一个新分支:

```vue
          <template v-else-if="item.icon === 'prompt'">
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
            <polyline points="14 2 14 8 20 8" />
            <line x1="16" y1="13" x2="8" y2="13" />
            <line x1="16" y1="17" x2="8" y2="17" />
            <polyline points="10 9 9 9 8 9" />
          </template>
```

- [x] **Step 2: 扩展 `Item` icon 联合类型**

把 `interface Item` 那行改为:

```ts
interface Item { to: string; label: string; icon: 'upload' | 'data' | 'convert' | 'prompt' | 'model' }
```

- [x] **Step 3: 在 `topItems` 数组插入 prompts 项**

把 `topItems` 数组改为:

```ts
const topItems: Item[] = [
  { to: '/uploads', label: '上传', icon: 'upload' },
  { to: '/data-assets', label: '数据资产', icon: 'data' },
  { to: '/transformations', label: '转换', icon: 'convert' },
  { to: '/prompts', label: '提示词', icon: 'prompt' },
  { to: '/models', label: '模型', icon: 'model' },
];
```

(prompts 插在 transformations 和 models 之间,跟 spec §3.1 一致)

- [x] **Step 4: 类型检查 + 视觉确认**

Run: `pnpm exec tsc --noEmit`
Expected: 无 error。

(视觉确认由 Task 11 一起做 — dev server 启动后看左侧导航。)

- [x] **Step 5: 提交**

```bash
git add src/components/Sidebar.vue
git commit -m "feat(sidebar): add prompts nav item + icon"
```

---

## Task 11: 全量验证 — 后端测试 + 前端测试 + build + dev 视觉

**Files:** none

- [x] **Step 1: 跑全部 Rust 测试**

Run: `cargo test -p nsc-core`
Expected: 全过,包括 Task 1 新增的 `count_by_prompt_*` 2 个测试 + 已有测试。

- [x] **Step 2: 跑 workspace 编译**

Run: `cargo build --workspace`
Expected: 成功,无 error/warning。

- [x] **Step 3: 跑前端类型检查**

Run: `pnpm exec tsc --noEmit`
Expected: 无 error。

- [x] **Step 4: 跑前端单元测试**

Run: `pnpm test`
Expected: 全过(99 个已有 + 11 个新增 prompts 测试 = 110 个左右)。

- [x] **Step 5: dev server 启动 + 视觉确认(后台启动,5s 后停)**

Run:
```bash
pnpm dev &
PID=$!
sleep 6
kill $PID 2>/dev/null
wait $PID 2>/dev/null
```
Expected: Vite 启动无错(在浏览器打开 http://localhost:43801 看 `/prompts` 路由,如果在沙箱环境看不到 UI,只看启动 log 没路由报错就行)。

- [x] **Step 6: 全无问题,合并 Task 1-10 的 commits(都在同一分支,无需 rebase)**

无 git 操作;若所有 commit 已在分支 `codex/upload-refactor` 上,直接继续。如果开发在 worktree / 其他分支,根据 executing-plans 约定决定。

---

## 自审检查清单

- [x] **Spec 覆盖:**
  - §3.1 路由 + Sidebar → Task 9 + Task 10
  - §3.2 组件树(Prompts.vue + PromptEditDialog.vue + Dialog 复用)→ Task 7 + Task 8
  - §3.3 Pinia store → Task 6
  - §3.4 数据流(新建 / 删除)→ Task 8
  - §4.1 5 个 IPC 命令 → Task 2 + Task 3
  - §4.2 PromptInput DTO → Task 2
  - §4.3 前端 IPC wrapper → Task 5
  - §5.1 `PromptRepo::count_by_prompt` → Task 1
  - §5.2 Tauri command 实现要点(锁 / upsert 分支)→ Task 2
  - §6.1 类型 → Task 4
  - §6.2 `Prompts.vue` → Task 8
  - §6.3 `PromptEditDialog.vue` (3 种 mode + 黄字警告)→ Task 7
  - §6.4 Sidebar 项 → Task 10
  - §7 校验与失败处理(delete confirm + N>0 提示)→ Task 8
  - §8.1 Backend 测试(count_by_prompt_returns_ref_count + count_by_prompt_zero_for_unused)→ Task 1
  - §8.2 Frontend 测试(IPC 形状 + store)→ Task 5 + Task 6
  - §10 验收标准 → Task 11

- [x] **Placeholder 扫描:** 无 TBD / TODO / "类似 Task N" / "implement later"。每步都有完整代码块。

- [x] **类型一致性:**
  - `Prompt` / `PromptInput` (Task 4) 与 store / wrapper / dialog / view (Task 5/6/7/8) 字段对齐
  - `count_by_prompt` (Task 1) 与 Tauri 命令 `count_transformation_chapters_by_prompt` (Task 2) 与 wrapper `countPromptUsage` (Task 5) 名称一致
  - `upsert_prompt` 分支条件 `payload.id == 0` (Task 2) 与 store `upsert` 调用 (Task 6) 与 dialog `id = mode === 'create' ? 0 : initial.id` (Task 7) 一致

- [x] **依赖顺序:** T1(repo) → T2(commands.rs) → T3(注册) → T4(types) → T5(wrapper 测试 + impl) → T6(store 测试 + impl) → T7(dialog) → T8(view) → T9(router) → T10(sidebar) → T11(verify)

---

## 实现背离(post-plan commits)

本计划执行完后,后续 UI 一致性重构覆盖了 Task 7 (Prompts.vue 顶部 header) 与 Task 10 (Sidebar.vue icon 渲染) 的实现细节。计划文件保留作为历史记录,实际代码以现 commit 为准。

### 1. Task 10 — Sidebar 图标从 inline SVG 切换为 lucide

- **计划写法**(1223-1277 行):Sidebar 用 `template v-else-if="item.icon === 'prompt'"` 写 inline SVG 分支,`Item.icon` 是字符串联合 `'upload' | 'data' | 'convert' | 'prompt' | 'model'`
- **实际写法**(commit `1a8df40`):改用 `unplugin-icons` + `@iconify-json/lucide`,`Item.icon` 变成 `Component` 类型,模板统一 `<component :is="item.icon" :size="16" :stroke-width="1.5" />`,`markRaw()` 防止组件被包成 reactive proxy
- **配套改动**:`vite.config.ts` 加 `Icons({ compiler: 'vue3', collections: ['lucide'] })`,新增 `src/types/icons.d.ts` 给 `~icons/*` shim
- **影响范围**:Sidebar 7 个图标 (upload / database / repeat / file-text / box / sun / moon),Upload / DataAsset 的返回按钮 `← 返回` 文字版换成 `<IconArrowLeft>` 图标版 (commit `f4342e7`)
- **意图**:vite 编译期按需生成图标组件 + tree-shake,避免手写 SVG 同步成本

### 2. Task 7 — Prompts.vue 顶部 header 收敛到 PageHeader 组件

- **计划写法**(957-962、1103-1111 行):`<header class="header"><h2>提示词</h2><div class="actions"><Button kind="primary">新建 prompt</Button></div></header>` + 视图内 scoped `.header` / `.header h2` / `.actions` CSS
- **实际写法**(commit `de79c58`):迁到 `<PageHeader title="提示词" subtitle="...">` 通用组件
- **PageHeader 组件**:三栏 grid (`auto 1fr auto`),槽位 `#back` / `#meta` / `#actions`,prop `title` / `subtitle` / `size`(default 22px / small 18px)。同次 commit 还覆盖了 Library / Upload / DataAsset / Models / parse 五个视图的 header,统一标题字号 / 字重 / 边框 / 对齐
- **后续微调**(commit `2d71b6a`):详情页(Upload / DataAsset)加 `size="small"`(18px,长文件名单行多装十几个字),并去掉冗余副标题;列表页(Library / Models / Prompts / parse)保持 default 22px + 副标题
- **意图**:6 个视图的 header 各写一套样式导致字号 22/24/28px 不一致、Models 误用 `--border-rouge` 分隔线、大按钮把行高撑开。统一后 CSS 总量 -1 KB

### 计划里其他仍准确的部分

- Task 1-9 的后端命令、IPC wrapper、store、router、dialog、view 实现与最终代码一致,只有 UI 渲染层 (Task 7 / Task 10) 被上述重构覆盖