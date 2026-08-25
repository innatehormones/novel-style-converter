# 章节边界真相源化 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让后端 splitter 输出的 `title_line` 成为章节边界的唯一真相源，前端零反查、零双轨（markers/suppressed）。

**Architecture:** 后端 `ParsedChapter` / DB `chapters.title_line` / IPC `ChapterSegment` 三处加 `title_line`（标题行 0-based 行号）；前端 `chapterSplits: Set<lineKey>` + `titles: Map<lineKey,title>` 两集合驱动，`applyWorking` 纯函数从 `rawLines` 切片派生 `workingChapters`，标题独立存取（不派生）。删除 markers/suppressed 双轨、`split_with_edits`、`parse_chapters` 死命令、前端反查函数。

**Tech Stack:** Rust（nsc-core + Tauri 命令）、rusqlite、Vue 3 + Pinia + TypeScript、vitest、cargo test。

**Spec:** `docs/superpowers/specs/2026-08-25-chapter-splits-unification-design.md`

---

## 文件结构

**后端（Rust）**
- `crates/nsc-core/src/splitter/rules.rs` — 加 `title_line`、fallback 统一化、删 markers 机制
- `crates/nsc-core/tests/splitter_new.rs` — 补 title_line 断言（已有 15 个空白容忍测试保留）
- `migrations/0028_chapter_title_line.sql` — 新增列
- `crates/nsc-core/src/db/migrate.rs` — 注册 0028
- `crates/nsc-core/src/models/chapter.rs` — `Chapter`/`NewChapter` 加 `title_line`
- `crates/nsc-core/src/db/repo/chapter.rs` — 读写 `title_line`、删 `replace_all_for_data_asset`
- `src-tauri/src/commands/chapters.rs` — `ChapterSegment`/`ChapterInput` 加字段、改 `list_chapter_segments`、删 `parse_chapters`
- `src-tauri/src/commands/data_assets.rs` — `commit_data_asset` 填 `title_line`
- `src-tauri/src/lib.rs` — 删 `parse_chapters` 注册

**前端（TS/Vue）**
- `src/ipc/types.ts` — `ChapterSegment`/`ChapterInput`/`CommitDataAssetInput` 加 `title_line`
- `src/ipc/commands.ts` — 删 `parseChapters`
- `src/utils/splitChapters.ts` — 删反查函数、加 `stripInvisibles`/`stripTrailingInvisibles`
- `src/stores/chapters.ts` — 栈化重构
- `src/views/parse.vue` — 删 merge、改命名、跳转用 `title_line`
- `src/composables/useParseEditor.ts` — 命名重构
- `src/__tests__/chapters.spec.ts` — 重写
- `src/__tests__/splitChapters.spec.ts` — 删反查用例、留工具用例

---

## Task 1: Rust splitter 加 `title_line` + fallback 统一化

**Files:**
- Modify: `crates/nsc-core/src/splitter/rules.rs`
- Test: `crates/nsc-core/tests/splitter_new.rs`

- [ ] **Step 1: 写失败测试（title_line 断言）**

在 `crates/nsc-core/tests/splitter_new.rs` 末尾追加：

```rust
#[test]
fn title_line_reported_for_regex_chapters() {
    let t = "第1章：我是好人\nbody1\n第2章：这是个误会\nbody2\n";
    let r = DefaultSplitter.split(t);
    let lines: Vec<usize> = r.chapters.iter().map(|c| c.title_line).collect();
    assert_eq!(lines, vec![0, 2], "title_line 应指向标题行, got {:?}", lines);
}

#[test]
fn title_line_with_leading_blank_lines() {
    // 前导空行:正则 [\s\p{Cf}]* 会跨行吃掉 \n,title_line 必须用 m.end() 前换行数,
    // 不能用 m.start()(会落在空行行首,少算)。
    let t = "\n\n第1章：我是好人\nbody1\n第2章：这是个误会\nbody2\n";
    let r = DefaultSplitter.split(t);
    let lines: Vec<usize> = r.chapters.iter().map(|c| c.title_line).collect();
    assert_eq!(lines, vec![2, 4], "前导空行会 off-by, got {:?}", lines);
}

#[test]
fn title_line_for_blank_line_fallback() {
    // 无章节标题 → 空行 fallback,title = 首行、content = 次行起。
    let t = "段落一标题\n段落一正文\n\n段落二标题\n段落二正文\n";
    let r = DefaultSplitter.split(t);
    assert_eq!(r.chapters.len(), 2);
    assert_eq!(r.chapters[0].title, "段落一标题");
    assert_eq!(r.chapters[0].title_line, 0);
    assert_eq!(r.chapters[0].content, "段落一正文");  // 首行已移出 content
    assert_eq!(r.chapters[1].title_line, 3);
    assert_eq!(r.chapters[1].content, "段落二正文");
}
```

- [ ] **Step 2: 运行确认失败**

```bash
cargo test -p nsc-core --test splitter_new
```
Expected: 编译失败（`ParsedChapter` 无 `title_line` 字段）。

- [ ] **Step 3: 改 `ParsedChapter` + `split()`**

`crates/nsc-core/src/splitter/rules.rs`：

```rust
#[derive(Debug, Clone)]
pub struct ParsedChapter {
    pub title: String,
    pub content: String,
    pub word_count: i32,
    pub title_line: usize,
}
```

`split()` 的两个分支（正则 + fallback）：

```rust
fn split(&self, text: &str) -> SplitResult {
    if text.trim().is_empty() { return SplitResult { chapters: vec![] }; }
    let matches = auto_matches_in(text);
    if matches.is_empty() {
        let mut chapters = Vec::new();
        let mut cursor = 0;
        for m in RE_BLANK_LINE.find_iter(text) {
            let (start, end) = (cursor, m.start());
            if start < end {
                let s = &text[start..end];
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    let (title, content) = split_first_line(trimmed);
                    let title_line = text[..start].matches('\n').count();
                    chapters.push(ParsedChapter { title, content: content.to_string(), word_count: word_count(&content), title_line });
                }
            }
            cursor = m.end();
        }
        if cursor < text.len() {
            let s = &text[cursor..];
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                let (title, content) = split_first_line(trimmed);
                let title_line = text[..cursor].matches('\n').count();
                chapters.push(ParsedChapter { title, content: content.to_string(), word_count: word_count(&content), title_line });
            }
        }
        return SplitResult { chapters };
    }
    let mut chapters = Vec::new();
    for (i, (_, end, title)) in matches.iter().enumerate() {
        let content_end = matches.get(i + 1).map(|(start, _, _)| *start).unwrap_or(text.len());
        let content = text[*end..content_end].trim().to_string();
        let title_line = text[..*end].matches('\n').count();
        if !content.is_empty() { chapters.push(ParsedChapter { title: title.clone(), word_count: word_count(&content), content, title_line }); }
    }
    SplitResult { chapters }
}
```

新增辅助函数（放在 `first_line_title` 附近）：

```rust
/// 把空行 fallback 的段落切成 (title=首行, content=次行起)。
/// 消除「正则路径 content 不含标题、fallback 含首行」的不一致。
/// 单行段落 → content 为空(确定性输出)。
fn split_first_line(s: &str) -> (String, String) {
    match s.find('\n') {
        Some(pos) => (s[..pos].trim().to_string(), s[pos + 1..].trim().to_string()),
        None => (s.to_string(), String::new()),
    }
}
```

- [ ] **Step 4: 删 markers 机制 + merge_suppressed**

同一文件：
- 删 `ChapterSplitter` trait 里的 `split_with_markers`（默认方法）和 `split_with_edits`（方法声明）。
- 删 `impl ChapterSplitter for DefaultSplitter` 里的 `split_with_edits` 整个方法体。
- 删文件末尾的 `fn merge_suppressed(...)`。
- `ChapterSplitter` trait 只剩：

```rust
pub trait ChapterSplitter: Send + Sync {
    fn split(&self, text: &str) -> SplitResult;
}
```

- [ ] **Step 5: 运行确认通过**

```bash
cargo test -p nsc-core --test splitter_new
cargo test -p nsc-core
```
Expected: 全绿（`cargo test -p nsc-core` 会因删了 `split_with_edits` 导致 `chapters.rs` 命令编译失败——这一步先只跑 splitter 测试，命令层在 Task 4 修）。

- [ ] **Step 6: Commit**

```bash
git add crates/nsc-core/src/splitter/rules.rs crates/nsc-core/tests/splitter_new.rs
git commit -m "feat(splitter): add title_line; unify blank-line fallback; drop markers mechanism"
```

---

## Task 2: DB migration + models 加 `title_line`

**Files:**
- Create: `migrations/0028_chapter_title_line.sql`
- Modify: `crates/nsc-core/src/db/migrate.rs`
- Modify: `crates/nsc-core/src/models/chapter.rs`

- [ ] **Step 1: 新增 migration 文件**

`migrations/0028_chapter_title_line.sql`：

```sql
-- chapter 加 title_line:标题文本在 upload.original_text 里的 0-based 行号。
-- NULL = 无原文坐标(仅 promote_workflow 转正的 AI 结果章节)。
-- 原始章节(fresh/committed)永远非 NULL。数据可清除,无 backfill。
ALTER TABLE chapters ADD COLUMN title_line INTEGER;
```

- [ ] **Step 2: 注册 migration**

`crates/nsc-core/src/db/migrate.rs` 的 `SCHEMAS` 数组末尾（`("0027_tc_batch_cascade", ...)` 之后）加：

```rust
    ("0028_chapter_title_line", include_str!("../../../../migrations/0028_chapter_title_line.sql")),
```

- [ ] **Step 3: models 加字段**

`crates/nsc-core/src/models/chapter.rs`：

```rust
pub struct Chapter {
    // ... 现有字段不变 ...
    pub edited_at: Option<String>,
    /// 标题文本在 upload.original_text 里的 0-based 行号。None = 无原文坐标(promoted)。
    #[serde(default)]
    pub title_line: Option<i32>,
}

pub struct NewChapter {
    // ... 现有字段不变 ...
    pub source_chapter_id: Option<i64>,
    pub title_line: Option<i32>,
}

impl Default for NewChapter {
    fn default() -> Self {
        Self {
            // ... 现有字段不变 ...
            source_chapter_id: None,
            title_line: None,
        }
    }
}
```

- [ ] **Step 4: 运行确认编译**

```bash
cargo build -p nsc-core
```
Expected: 编译失败（`chapter.rs` 的 `chapter_from_row` / INSERT 未处理 title_line——Task 3 修）。先确认 `migrate.rs` 无语法错误（可 `cargo test -p nsc-core --lib` 单独跑，会因 repo 未改而失败，忽略）。

- [ ] **Step 5: Commit**

```bash
git add migrations/0028_chapter_title_line.sql crates/nsc-core/src/db/migrate.rs crates/nsc-core/src/models/chapter.rs
git commit -m "feat(db): add chapter.title_line column + model field"
```

---

## Task 3: repo 读写 `title_line` + 删 `replace_all_for_data_asset`

**Files:**
- Modify: `crates/nsc-core/src/db/repo/chapter.rs`

- [ ] **Step 1: `chapter_from_row` 读第 10 列**

```rust
fn chapter_from_row(row: &Row<'_>) -> rusqlite::Result<Chapter> {
    Ok(Chapter {
        id: row.get(0)?,
        data_asset_id: row.get(1)?,
        idx: row.get(2)?,
        title: row.get(3)?,
        body: row.get(4)?,
        word_count: row.get(5)?,
        source_chapter_id: row.get(6)?,
        source_kind: row.get(7)?,
        edited_at: row.get(8)?,
        title_line: row.get(9)?,
    })
}
```

- [ ] **Step 2: 所有 SELECT 加 `title_line` 列**

四个 SELECT（`list_by_data_asset` / `get` / `prev_n` / `next_n`）的列清单从：

```sql
SELECT id, data_asset_id, idx, title, body, word_count, source_chapter_id, source_kind, edited_at
```

统一改成：

```sql
SELECT id, data_asset_id, idx, title, body, word_count, source_chapter_id, source_kind, edited_at, title_line
```

- [ ] **Step 3: `insert` 加 `title_line` 列**（promotion.rs 生产调用方，保留）

```rust
pub fn insert(&self, c: &NewChapter) -> Result<i64> {
    self.conn.execute(
        "INSERT INTO chapters (data_asset_id, idx, title, body, word_count, source_kind, source_chapter_id, title_line) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![c.data_asset_id, c.idx, c.title, c.body, c.word_count, c.source_kind.clone(), c.source_chapter_id, c.title_line],
    )?;
    Ok(self.conn.last_insert_rowid())
}
```

- [ ] **Step 4: `insert_many` 加 `title_line` 列**（commit_data_asset 落库路径）

```rust
pub fn insert_many(&self, data_asset_id: i64, items: &[NewChapter]) -> Result<()> {
    let tx = self.conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO chapters (data_asset_id, idx, title, body, word_count, title_line) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for c in items {
            stmt.execute(params![data_asset_id, c.idx, c.title, c.body, c.word_count, c.title_line])?;
        }
    }
    tx.commit()?;
    Ok(())
}
```

- [ ] **Step 5: 删 `replace_all_for_data_asset`**

删除 `pub fn replace_all_for_data_asset(&self, ...) -> Result<usize> { ... }` 整个方法（仅死命令 `parse_chapters` 用，Task 4 删命令）。

- [ ] **Step 6: 运行确认编译**

```bash
cargo build -p nsc-core
```
Expected: 仍可能失败——`data_assets.rs` / `chapters.rs` 里 `NewChapter` 构造处缺 `title_line`（用 `..Default::default()` 的不受影响），以及 `chapters.rs` 调 `replace_all_for_data_asset` / `split_with_edits`。Task 4 修命令层。确认本文件无语法错误。

- [ ] **Step 7: Commit**

```bash
git add crates/nsc-core/src/db/repo/chapter.rs
git commit -m "feat(db): read/write chapter.title_line; drop replace_all_for_data_asset"
```

---

## Task 4: IPC 命令层

**Files:**
- Modify: `src-tauri/src/commands/chapters.rs`
- Modify: `src-tauri/src/commands/data_assets.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: `ChapterSegment` / `ChapterInput` 加 `title_line`**

`src-tauri/src/commands/chapters.rs`：

```rust
#[derive(Debug, Serialize)]
pub struct ChapterSegment {
    pub title: String,
    pub content: String,
    pub word_count: i32,
    pub title_line: i32,
}

#[derive(Debug, Deserialize)]
pub struct ChapterInput {
    pub title: String,
    pub content: String,
    pub title_line: i32,
}
```

- [ ] **Step 2: `list_chapter_segments` 删 markers/suppressed 参数、改调 `split()`**

```rust
#[tauri::command]
pub fn list_chapter_segments(
    db: State<'_, Arc<Db>>,
    upload_id: i64,
) -> Result<Vec<ChapterSegment>, String> {
    let text = {
        let u = db.uploads().get(upload_id).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("upload {upload_id} 不存在"))?;
        crate::commands::uploads::read_upload_original_text(&u)?
    };
    let SplitResult { chapters } = DefaultSplitter.split(&text);
    Ok(chapters.into_iter().map(|c| ChapterSegment {
        title: c.title,
        content: c.content,
        word_count: c.word_count,
        title_line: c.title_line as i32,
    }).collect())
}
```

- [ ] **Step 3: `list_committed_segments` 读 `title_line`（None 报错）**

```rust
#[tauri::command]
pub fn list_committed_segments(
    db: State<'_, Arc<Db>>,
    data_asset_id: i64,
) -> Result<Vec<ChapterSegment>, String> {
    let chapters = db.chapters().list_by_data_asset(data_asset_id).map_err(|e| e.to_string())?;
    chapters.into_iter().map(|c| {
        let title_line = c.title_line
            .ok_or_else(|| format!("chapter {} title_line 为 NULL(data_asset_id={data_asset_id})", c.id))?;
        Ok(ChapterSegment { title: c.title, content: c.body, word_count: c.word_count, title_line })
    }).collect()
}
```

- [ ] **Step 4: 删 `parse_chapters`**

删除 `chapters.rs` 里的 `pub fn parse_chapters(...)` 整个函数。

- [ ] **Step 5: `commit_data_asset` 填 `title_line`**

`src-tauri/src/commands/data_assets.rs` 的 `NewChapter` 构造处加一行：

```rust
NewChapter {
    data_asset_id: da_id,
    idx: (i + 1) as i32,
    title: c.title,
    body: c.content,
    word_count: wc,
    title_line: Some(c.title_line),
    ..Default::default()
}
```

- [ ] **Step 6: 删 `lib.rs` 里的注册**

`src-tauri/src/lib.rs` 删这一行：

```rust
commands::chapters::parse_chapters,
```

- [ ] **Step 7: 运行确认编译 + 测试**

```bash
cargo build
cargo test -p nsc-core
```
Expected: 全绿。`cargo build` 编译通过（命令层不再引用 `split_with_edits` / `replace_all_for_data_asset` / `parse_chapters`）。

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands/chapters.rs src-tauri/src/commands/data_assets.rs src-tauri/src/lib.rs
git commit -m "feat(ipc): thread title_line through chapter commands; drop parse_chapters"
```

---

## Task 5: 前端 types 加 `title_line`

**Files:**
- Modify: `src/ipc/types.ts`
- Modify: `src/ipc/commands.ts`

- [ ] **Step 1: types 加字段**

`src/ipc/types.ts`：

```ts
export interface ChapterSegment {
  title: string;
  content: string;
  word_count: number;
  title_line: number;      // 标题行 0-based 行号
  edited_at?: string | null;
}

export type ChapterInput = {
  title: string;
  content: string;
  title_line: number;
};

export interface CommitDataAssetInput {
  title: string;
  chapters: Array<{
    title: string;
    content: string;
    title_line: number;
  }>;
}
```

- [ ] **Step 2: 删 `parseChapters`**

`src/ipc/commands.ts` 删：

```ts
export function parseChapters(dataAssetId: number, segments: ChapterInput[]): Promise<number> {
  return invoke<number>('parse_chapters', { dataAssetId, segments });
}
```

并从文件顶部 import 列表里删掉 `ChapterInput`（若不再被引用）。

- [ ] **Step 3: 运行确认类型检查**

```bash
pnpm exec vue-tsc --noEmit
```
Expected: 报错集中在 `chapters.ts` store（`commit` 里 `ChapterInput` 缺 `title_line`）、`splitChapters.ts`（`ChapterSegment` 缺 `title_line` 的构造）——这些在 Task 6/7 修。确认 `types.ts` / `commands.ts` 无语法错误。

- [ ] **Step 4: Commit**

```bash
git add src/ipc/types.ts src/ipc/commands.ts
git commit -m "feat(ipc): add title_line to frontend types; drop parseChapters"
```

---

## Task 6: splitChapters.ts 清理

**Files:**
- Modify: `src/utils/splitChapters.ts`
- Test: `src/__tests__/splitChapters.spec.ts`

- [ ] **Step 1: 重写 `splitChapters.ts`**

删 `splitChaptersByMarkers` / `diagnoseSplit` / `bodyStartByTitle` / `bodyStartByContent` / `parseChapterTitleStrict` / `parseChapterTitle` / `findHitsInRange`。保留 `escapeRegExp` / `INVIS_PREFIX_RE` / `INVIS_SUFFIX_RE` / `isVisuallyEmptyLine` / `countChapterChars`。新增两个工具：

```ts
import type { ChapterSegment } from '../ipc/types';

/// 转义正则特殊字符(保留,给别的调用方用)。
function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^\${()|[\]\\]/g, '\\$&');
}

export const INVIS_PREFIX_RE = /^[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+/u;
export const INVIS_SUFFIX_RE = /[\s\u{200B}\u{200C}\u{200D}\u{2060}\u{FEFF}]+$/u;

export function isVisuallyEmptyLine(line: string): boolean {
  return line.replace(INVIS_PREFIX_RE, '').replace(INVIS_SUFFIX_RE, '') === '';
}

export function countChapterChars(s: string): number {
  let n = 0;
  for (const ch of s) if (!/\s/.test(ch)) n++;
  return n;
}

/// 去掉首尾 whitespace + invisible 格式字符。
export function stripInvisibles(s: string): string {
  return s.replace(INVIS_PREFIX_RE, '').replace(INVIS_SUFFIX_RE, '');
}

/// 只去掉末尾的 whitespace + invisible(内容前导空白保留,与 splitter trim 语义对齐)。
export function stripTrailingInvisibles(s: string): string {
  return s.replace(INVIS_SUFFIX_RE, '');
}
```

注意：原文件里 `escapeRegExp` 的替换串是坏的（`'\\\\/// zh-aware 字数简化版'`），本任务顺带修正为 `'\\$&'`。

- [ ] **Step 2: 删测试里的反查用例**

`src/__tests__/splitChapters.spec.ts` 删 `splitChaptersByMarkers` / `diagnoseSplit` 相关 describe/用例，保留 `isVisuallyEmptyLine` / `countChapterChars` 用例。

- [ ] **Step 3: 运行确认**

```bash
pnpm test src/__tests__/splitChapters.spec.ts
```
Expected: 保留用例全绿。

- [ ] **Step 4: Commit**

```bash
git add src/utils/splitChapters.ts src/__tests__/splitChapters.spec.ts
git commit -m "refactor(split): remove reverse-lookup fns; keep strip/count helpers"
```

---

## Task 7: store 栈化重构（核心）

**Files:**
- Modify: `src/stores/chapters.ts`
- Test: `src/__tests__/chapters.spec.ts`

- [ ] **Step 1: 重写测试（先写失败测试）**

`src/__tests__/chapters.spec.ts` 整体替换。`SEGMENTS` 加 `title_line`，mock 不变：

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';

const TEXT = "content intro\n\n第一章：开篇\nbody1 line 1\nbody1 line 2\nbody1 line 3\n第二章今世只想生孩子\nbody2 line 1\nbody2 line 2\nbody2 line 3\n第三章：误会\nbody3 line 1\nbody3 line 2";

vi.mock('../ipc/commands', () => ({
  getUploadText: vi.fn(async () => TEXT),
  getUpload: vi.fn(async (id) => ({ id, filename: 'sample.txt', size: TEXT.length })),
  findDataAssetByUpload: vi.fn(async () => []),
  listChapterSegments: vi.fn(async () => SEGMENTS),
  listCommittedSegments: vi.fn(async () => []),
  commitDataAsset: vi.fn(async () => 1),
}));

vi.mock('@vueuse/core', () => ({
  useDebounceFn: (fn: (...args: unknown[]) => unknown) => fn,
}));

import { useChaptersStore } from '../stores/chapters';

// title_line = 标题行 0-based 行号(见 TEXT)。
const SEGMENTS = [
  { title: '第一章：开篇', content: 'body1 line 1\nbody1 line 2\nbody1 line 3', word_count: 6, title_line: 2 },
  { title: '第二章今世只想生孩子', content: 'body2 line 1\nbody2 line 2\nbody2 line 3', word_count: 6, title_line: 6 },
  { title: '第三章：误会', content: 'body3 line 1\nbody3 line 2', word_count: 6, title_line: 10 },
];

beforeEach(() => { setActivePinia(createPinia()); });
afterEach(() => { vi.clearAllMocks(); });

describe('chapters store: 栈化 chapterSplits', () => {
  it('load 用 title_line 初始化 chapterSplits 与 titles', async () => {
    const store = useChaptersStore();
    await store.load(1);
    expect([...store.chapterSplits].map(Number).sort((a,b)=>a-b)).toEqual([2, 6, 10]);
    expect(store.titles.get('6')).toBe('第二章今世只想生孩子');
    expect(store.workingChapters.map((c) => c.title)).toEqual(['第一章：开篇', '第二章今世只想生孩子', '第三章：误会']);
    expect(store.dirty).toBe(false);
  });

  it('toggleChapterSplit 删标题行 → 该章并入上一章', async () => {
    const store = useChaptersStore();
    await store.load(1);
    store.toggleChapterSplit('6');
    expect(store.workingChapters.map((c) => c.title)).toEqual(['第一章：开篇', '第三章：误会']);
    // 第一章 content 现在包含原第二章 body。
    expect(store.workingChapters[0].content).toContain('body2 line 1');
    expect(store.dirty).toBe(true);
  });

  it('toggleChapterSplit 加 body 行 → 切出新章', async () => {
    const store = useChaptersStore();
    await store.load(1);
    store.toggleChapterSplit('4');  // body1 line 2 行
    expect(store.workingChapters.length).toBe(4);
    expect(store.workingChapters[1].title).toBe('body1 line 2');
  });

  it('同一行 toggle 两次净变化 0 + 标题恢复', async () => {
    const store = useChaptersStore();
    await store.load(1);
    store.toggleChapterSplit('6');
    store.toggleChapterSplit('6');
    expect(store.workingChapters.map((c) => c.title)).toEqual(['第一章：开篇', '第二章今世只想生孩子', '第三章：误会']);
    expect(store.dirty).toBe(false);
  });

  it('updateTitle 改标题只改 title 不改 content', async () => {
    const store = useChaptersStore();
    await store.load(1);
    const before = store.workingChapters[1].content;
    store.updateTitle('6', '楔子');
    expect(store.workingChapters[1].title).toBe('楔子');
    expect(store.workingChapters[1].content).toBe(before);
    expect(store.dirty).toBe(true);
  });

  it('reset 恢复 initialChapterSplits + initialTitles', async () => {
    const store = useChaptersStore();
    await store.load(1);
    store.toggleChapterSplit('4');
    store.updateTitle('6', '楔子');
    store.reset();
    expect([...store.chapterSplits].map(Number).sort((a,b)=>a-b)).toEqual([2, 6, 10]);
    expect(store.titles.get('6')).toBe('第二章今世只想生孩子');
    expect(store.dirty).toBe(false);
  });
});
```

- [ ] **Step 2: 运行确认失败**

```bash
pnpm test src/__tests__/chapters.spec.ts
```
Expected: FAIL（`store.chapterSplits` / `toggleChapterSplit` / `updateTitle` / `store.titles` 不存在）。

- [ ] **Step 3: 重写 store**

`src/stores/chapters.ts` 整体替换（保留 import 与 `SourceKind` 相关，删 markers/suppressed/segLineMap）：

```ts
import { defineStore } from 'pinia';
import { useDebounceFn } from '@vueuse/core';
import { computed, ref } from 'vue';
import type { ChapterSegment, ChapterInput } from '../ipc/types';
import {
  commitDataAsset as ipcCommitDataAsset,
  findDataAssetByUpload as ipcFindDataAssetByUpload,
  getUploadText as ipcGetUploadText,
  getUpload as ipcGetUpload,
  listChapterSegments as ipcListChapterSegments,
  listCommittedSegments as ipcListCommittedSegments,
} from '../ipc/commands';
import { countChapterChars, stripInvisibles, stripTrailingInvisibles } from '../utils/splitChapters';

type SourceKind = 'committed' | 'fresh';

export const useChaptersStore = defineStore('chapters', () => {
  const uploadId = ref<number | null>(null);
  const rawText = ref<string>('');
  const filename = ref<string>('');

  const rawLines = computed<string[]>(() => rawText.value.split('\n'));

  const chapterSplits = ref<Set<string>>(new Set());
  const initialChapterSplits = ref<Set<string>>(new Set());
  const titles = ref<Map<string, string>>(new Map());
  const initialTitles = ref<Map<string, string>>(new Map());

  const workingChapters = ref<ChapterSegment[]>([]);

  const loading = ref(false);
  const error = ref<string | null>(null);
  const sourceKind = ref<SourceKind | null>(null);

  let requestToken = 0;

  const committed = computed(() => sourceKind.value === 'committed');

  function setEqual(a: Set<string>, b: Set<string>): boolean {
    if (a.size !== b.size) return false;
    for (const k of a) if (!b.has(k)) return false;
    return true;
  }
  function mapEqual(a: Map<string, string>, b: Map<string, string>): boolean {
    if (a.size !== b.size) return false;
    for (const [k, v] of a) if (b.get(k) !== v) return false;
    return true;
  }

  const dirty = computed(() =>
    !setEqual(chapterSplits.value, initialChapterSplits.value) ||
    !mapEqual(titles.value, initialTitles.value),
  );

  function applyWorking(): ChapterSegment[] {
    const sortedKeys = [...chapterSplits.value].map(Number).sort((a, b) => a - b);
    const out: ChapterSegment[] = [];
    for (let i = 0; i < sortedKeys.length; i++) {
      const key = sortedKeys[i];
      const next = i + 1 < sortedKeys.length ? sortedKeys[i + 1] : rawLines.value.length;
      const title = titles.value.get(String(key));
      if (title === undefined) {
        throw new Error(`titles 缺 key=${key}：chapterSplits 与 titles 不一致`);
      }
      const content = rawLines.value.slice(key + 1, next).join('\n');
      out.push({ title, content: stripTrailingInvisibles(content), word_count: countChapterChars(content), title_line: key });
    }
    return out;
  }

  function recompute() { workingChapters.value = applyWorking(); }

  function recomputeInitialFromSegs(segs: ChapterSegment[]) {
    const splits = new Set<string>();
    const t = new Map<string, string>();
    for (const s of segs) {
      if (s.title_line < 0 || s.title_line >= rawLines.value.length) {
        throw new Error(`title_line 越界：upload_id=${uploadId.value} title_line=${s.title_line} rawLines=${rawLines.value.length}`);
      }
      splits.add(String(s.title_line));
      t.set(String(s.title_line), s.title);
    }
    initialChapterSplits.value = splits;
    initialTitles.value = t;
    chapterSplits.value = new Set(splits);
    titles.value = new Map(t);
  }

  function toggleChapterSplit(key: string) {
    if (chapterSplits.value.has(key)) {
      chapterSplits.value.delete(key);
      titles.value.delete(key);
    } else {
      const line = Number(key);
      if (!Number.isFinite(line) || line < 0 || line >= rawLines.value.length) {
        throw new Error(`toggleChapterSplit 越界：key=${key} rawLines=${rawLines.value.length}`);
      }
      chapterSplits.value.add(key);
      if (!titles.value.has(key)) titles.value.set(key, stripInvisibles(rawLines.value[line]));
    }
    recompute();
  }

  function updateTitle(key: string, title: string) {
    if (!titles.value.has(key)) throw new Error(`updateTitle 未知 key=${key}`);
    titles.value.set(key, title);
    recompute();
  }

  function reset() {
    chapterSplits.value = new Set(initialChapterSplits.value);
    titles.value = new Map(initialTitles.value);
    recompute();
  }

  async function load(id: number) {
    uploadId.value = id;
    workingChapters.value = [];
    rawText.value = '';
    filename.value = '';
    chapterSplits.value = new Set();
    initialChapterSplits.value = new Set();
    titles.value = new Map();
    initialTitles.value = new Map();
    sourceKind.value = null;
    loading.value = true;
    error.value = null;
    ++requestToken;
    const token = requestToken;
    try {
      const [text, meta, dataAssetIds] = await Promise.all([
        ipcGetUploadText(id),
        ipcGetUpload(id).catch(() => null),
        ipcFindDataAssetByUpload(id).catch(() => [] as number[]),
      ]);
      if (token !== requestToken) return;
      rawText.value = text;
      filename.value = meta?.filename ?? '';
      const ownedDataAssetId = dataAssetIds[0] ?? null;
      const segs = ownedDataAssetId !== null
        ? await ipcListCommittedSegments(ownedDataAssetId)
        : await ipcListChapterSegments(id);
      if (token !== requestToken) return;
      sourceKind.value = ownedDataAssetId !== null ? 'committed' : 'fresh';
      recomputeInitialFromSegs(segs);
      recompute();
    } catch (e: unknown) {
      if (token === requestToken) error.value = e instanceof Error ? e.message : String(e);
    } finally {
      if (token === requestToken) loading.value = false;
    }
  }

  function unload() {
    uploadId.value = null;
    rawText.value = '';
    filename.value = '';
    chapterSplits.value = new Set();
    initialChapterSplits.value = new Set();
    titles.value = new Map();
    initialTitles.value = new Map();
    workingChapters.value = [];
    sourceKind.value = null;
    error.value = null;
    loading.value = false;
  }

  async function reSplit() {
    if (uploadId.value === null || sourceKind.value !== 'committed') return;
    const id = uploadId.value;
    const token = ++requestToken;
    try {
      const fresh = await ipcListChapterSegments(id);
      if (token !== requestToken) return;
      sourceKind.value = 'fresh';
      recomputeInitialFromSegs(fresh);
      recompute();
    } catch (e: unknown) {
      if (token === requestToken) error.value = e instanceof Error ? e.message : String(e);
    }
  }

  async function commit(title: string): Promise<number> {
    if (uploadId.value === null) throw new Error('no upload loaded');
    const segs: ChapterInput[] = workingChapters.value.map((s) => ({
      title: s.title,
      content: s.content,
      title_line: s.title_line,
    }));
    try {
      const newDataAssetId = await ipcCommitDataAsset(uploadId.value, { title, chapters: segs });
      sourceKind.value = 'committed';
      initialChapterSplits.value = new Set(chapterSplits.value);
      initialTitles.value = new Map(titles.value);
      return newDataAssetId;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    }
  }

  return {
    uploadId, rawText, filename, rawLines,
    workingChapters, chapterSplits, initialChapterSplits, titles, initialTitles,
    sourceKind, loading, error, committed, dirty,
    load, toggleChapterSplit, updateTitle, reset, reSplit, commit, unload,
  };
});
```

- [ ] **Step 4: 运行确认通过**

```bash
pnpm test src/__tests__/chapters.spec.ts
```
Expected: 全绿。

- [ ] **Step 5: 全量前端测试确认无回归**

```bash
pnpm test
```
Expected: 除 parse.vue / useParseEditor 相关（Task 8/9 修）外全绿。若有 `commands.spec.ts` 断言了 `parse_chapters` mock，一并删除。

- [ ] **Step 6: Commit**

```bash
git add src/stores/chapters.ts src/__tests__/chapters.spec.ts
git commit -m "refactor(chapters): replace markers/suppressed with chapterSplits stack"
```

---

## Task 8: parse.vue 重构

**Files:**
- Modify: `src/views/parse.vue`

- [ ] **Step 1: 删 merge 相关**

删模板里的 merge `<Dialog>`、`pendingMerge` / `mergeDialogOpen` 状态、`onMergeClick` / `cancelMerge` / `confirmMerge` 函数、左侧「并入上一章」按钮。

- [ ] **Step 2: 改命名与跳转**

- `markerSet` computed → `boundarySet`（读 `store.chapterSplits`）。
- `onMarkerToggle(line1based)` → `onBoundaryToggle(line1based)`，函数体改为 `store.toggleChapterSplit(String(line1based - 1))`（CM6 1-based → store 0-based）。
- 章节点击跳转：`scrollToLine(chapter.title_line)`，删 `startLineOf` 调用。

- [ ] **Step 3: 运行确认编译**

```bash
pnpm exec vue-tsc --noEmit
```
Expected: parse.vue 无类型错误（useParseEditor 的 `opts.markerSet` 改名在 Task 9 同步）。

- [ ] **Step 4: Commit**

```bash
git add src/views/parse.vue
git commit -m "refactor(parse): drop merge dialog; drive boundary from chapterSplits"
```

---

## Task 9: useParseEditor.ts 命名重构

**Files:**
- Modify: `src/composables/useParseEditor.ts`

- [ ] **Step 1: 改名（不改行为 / CSS class）**

- `UseParseEditorOptions.markerSet` → `boundarySet`
- `syncMarkedClass` → `syncBoundaryClass`
- `markerLineDeco` → `boundaryLineDeco`
- `markerEffect` / `markerField` / `MarkerStamp` → `boundaryEffect` / `boundaryField` / `BoundaryStamp`（内部命名）
- CSS class 名（`cm-marker-stamp` / `cm-marker-line` / `cm-marker-gutter` / `cm-marker-stamp--marked`）**保持不变**。

- [ ] **Step 2: 运行确认编译**

```bash
pnpm exec vue-tsc --noEmit
```
Expected: 无类型错误。

- [ ] **Step 3: 全量前端测试**

```bash
pnpm test
```
Expected: 全绿。

- [ ] **Step 4: Commit**

```bash
git add src/composables/useParseEditor.ts
git commit -m "refactor(parse): rename marker* to boundary* in CM6 editor"
```

---

## Task 10: e2e 断言更新（placeholder）

**Files:**
- Modify: `tests-e2e/library.spec.ts`（或 parse 页相关 spec，若不存在则跳过）

- [ ] **Step 1: 更新 parse 页相关断言**

E2E 当前是 `test.skip` placeholder（需真实 Tauri 运行时 + fake LLM，本地无法跑）。本任务仅保证 spec 里的 e2e 语义（body 行 toggle、标题行 toggle、reset 恢复）在注释/断言里对齐栈化模型，不实际执行。若现有 spec 无 parse 页用例，则此任务可跳过，记录在 spec 附注。

- [ ] **Step 2: Commit（如改动了文件）**

```bash
git add tests-e2e/library.spec.ts
git commit -m "test(e2e): align parse assertions with chapterSplits stack"
```

---

## Self-Review 结论

- **Spec 覆盖**：§3.1→Task 1；§3.2/3.3→Task 2；§3.4→Task 3；§3.5/3.6→Task 4；§4.1→Task 5；§4.3→Task 6；§4.2→Task 7；§4.4→Task 8；§4.5→Task 9；§7.3→Task 10。§5 fail-fast 契约落在 Task 3（None 报错）、Task 4（list_committed_segments 报错）、Task 7（越界/缺 key 抛错）。
- **保存校验**（spec §6 拆出）：本计划不含，另开独立 spec。
- **类型一致性**：`title_line` 后端 `Option<i32>`/`i32`、前端 `number`；`toggleChapterSplit` / `updateTitle` / `recomputeInitialFromSegs` / `chapterSplits` / `titles` 命名跨 Task 7/8/9 一致。
