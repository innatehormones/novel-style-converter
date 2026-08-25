# 章节边界真相源化设计

**状态**: 待用户审批
**日期**: 2026-08-25

---

## 1. 背景

### 1.1 痛点

用户在章节解析页操作章节边界时，视觉与数据不自洽：

1. 点「章」新建章节，再用「并入上一章」撤销，撤销后右侧章按钮仍绿色、行底色残留。
2. 在已合并章节的标题行点「章」，该章节不回来（复活失效）。
3. 保存前没有任何可读的风险提示（0 章、单章超 3 万、多章超 1 万）。

### 1.2 根因（为何旧 spec 只治标）

旧 spec（栈化 `chapterSplits` + 保存校验）方向对，但没治到根。真正的根因是**「章节边界」没有单一可靠真相源**，前端靠 `indexOf(title)` / `indexOf(content)` 反查 rawText 行号来把章节映射回原文：

- `markers`（按行号）与 `suppressed`（按 `seg.content`）双轨，`segmentKey = seg.content` 在标题编辑后不稳定 → 那一堆 guard（`mergeSuppressed` 首段 continue、`findSuppressedSegAtLine` 反查、zombie 防御、`removeChapter(idx===0)`）全是双轨的补丁。
- `bodyStartByTitle`/`bodyStartByContent` 反查本身脆弱：`bodyStartByContent` 注释自己承认「`segs[0]` (4824 字) 在 text 里 indexOf 返回 -1」。committed 状态用户改过标题后，`bodyStartByTitle` 必失败、`bodyStartByContent` 也失败 → 章节静默丢失。
- `bodyStartByTitle` 返回的是 **body 首行**（标题行的下一行），不是标题行 —— 旧 spec 拿它当「标题行号」，是 off-by-one。

### 1.3 业务目标

让**后端 splitter 输出的标题行号 `title_line` 成为章节边界的唯一真相源**，前端不再反查、不再双轨：

- 后端 `ChapterSegment` / DB `chapters.title_line` / 前端 `ChapterSegment` 都带 `title_line`（标题文本所在行，0-based）。
- 前端 `chapterSplits: Set<lineKey>` 由 `title_line` 初始化，运行时纯函数派生 `workingChapters`，标题独立存取（不派生）。
- commit 落库 `title_line`，committed 状态再 load 无损恢复，无任何反查。

### 1.4 硬约束（用户指示）

- **数据可随时清除**（除模型 / prompt 外）→ 不做迁移兼容、不 backfill、不写旧数据 NULL 兜底。旧 DB 删掉重测。
- **不过度兜底** → 哪里错哪里报错，带诊断信息；`title_line` 缺失/越界、`titles` 缺 key 直接报错，不 `??` 兜底。
- **不缝补** → markers/suppressed 双轨、`split_with_edits` 的 markers 机制、`parse_chapters` 死命令、反查函数全部删除，不留死代码。

---

## 2. 核心模型

```
load fresh:     后端 split() 输出 [{title_line, title, content}]
                → chapterSplits = Set(title_line)
                → titles = Map(title_line → title)   // 全量，非 override-only
load committed: 从库读 [{title_line, title, content}] → 同上
用户 toggle:    toggleChapterSplit(key) 唯一写入口（对称 add/delete）
派生:           workingChapters = applyWorking(chapterSplits, titles, rawLines)  // 纯函数
commit:         commit_data_asset({title, chapters: [{title_line, title, content}]}) 落库
```

`applyWorking`（纯函数，无兜底）：

```
sortedKeys = [...chapterSplits].map(Number).sort((a,b)=>a-b)
for i, key of sortedKeys:
  next = sortedKeys[i+1] ?? rawLines.length
  title   = titles.get(key)      // 缺失即 bug，见 §5 fail-fast
  content = rawLines.slice(key+1, next).join('\n').stripTrailingInvisibles()
  out.push({ title, content, title_line: key, word_count: countChapterChars(content) })
```

- `title` 全量存 `titles`，因为 committed 状态用户改过标题后 `rawLines[key]` 是旧标题，不能派生。
- 边界语义：`lineKey` = **标题行**，`content` 从 `lineKey+1` 开始（标题行不进正文）。

---

## 3. 后端改动

### 3.1 Rust splitter（`crates/nsc-core/src/splitter/rules.rs`）

- `ParsedChapter` 加 `title_line: usize`。
- `split()` 里对每个章节算标题行号（0-based）：
  - **正则路径**：`title_line = text[..m.end()].matches('\n').count()`。**必须用 `m.end()`，不能用 `m.start()`** —— 正则 `(?m)^[\s\p{Cf}]*第…` 里 `\s` 含 `\n`，前导空行会被 `[\s\p{Cf}]*` 吃掉，`m.start()` 落在空行行首而非「第」字所在行，会少算行号。
  - **空行 fallback 路径统一化**：现在 `content = s.trim()`（含首行），改成 `title = 首行`、`content = 次行起`，消除「正则路径 content 不含标题、fallback 含首行」的不一致。`title_line = 段落首行在全文中的行号`。退化边界：段落只有一行时 `content` 为空（确定性输出，非兜底），记录为已知边界。
- 删除 `split_with_edits` / `split_with_markers`（trait 方法 + 默认实现）—— markers 机制整体废弃（grep 确认仅 `list_chapter_segments` 调用）。
- 删除空实现占位的 `merge_suppressed`。

### 3.2 DB migration（`migrations/0028_chapter_title_line.sql`）

```sql
ALTER TABLE chapters ADD COLUMN title_line INTEGER;
```

- **可空、无 DEFAULT**。`NULL` 语义 = **无原文坐标**（仅 promoted 章节，即 workflow 转正的 AI 结果，见 §3.4）。
- 原始章节（fresh / committed）`title_line` 永远非 NULL（splitter 必输出）。
- 数据可清除，旧 DB 删掉重测，无 backfill、无 NULL 兜底。

### 3.3 models（`crates/nsc-core/src/models/chapter.rs`）

- `Chapter` 加 `title_line: Option<i32>`。
- `NewChapter` 加 `title_line: Option<i32>`，`Default` 设 `None`。

### 3.4 repo（`crates/nsc-core/src/db/repo/chapter.rs`）

- `chapter_from_row` 读第 10 列 `title_line`。
- 所有 `SELECT`（`list_by_data_asset` / `get` / `prev_n` / `next_n`）列清单加 `title_line`。
- `insert`（7 列版，含 source_kind/source_chapter_id）：**保留**，加 `title_line` 列 —— 调用方是 `promotion.rs::create_promoted_from_workflow`（`promote_workflow` 命令），promoted 章节 `title_line = None`。
- `insert_many`（5 列版）：加 `title_line` 列 —— 调用方 `commit_data_asset`，原始章节 `title_line = Some(line)`。
- `replace_all_for_data_asset`：随 §3.6 死代码删除。

### 3.5 IPC 命令（`src-tauri/src/commands/chapters.rs` + `data_assets.rs`）

- `ChapterSegment`（chapters.rs）加 `title_line: i32`（非空，parse 页专用）。
- `ChapterInput`（chapters.rs）加 `title_line: i32`。
- `list_chapter_segments`：删 `markers`/`suppressed` 参数，改调 `DefaultSplitter.split(&text)`，返回带 `title_line`。
- `list_committed_segments`：从 `chapter.title_line` 读，`None` 直接报错（fail-fast，见 §5）。
- `commit_data_asset`（data_assets.rs）：`NewChapter.title_line = Some(c.title_line)`。

### 3.6 死代码删除

- `parse_chapters`（chapters.rs）+ 注册处（lib.rs:133）+ `replace_all_for_data_asset`（repo）：前端 chapters.ts 的 commit 走 `commit_data_asset`，`parse_chapters` 已无调用方（grep 确认）。删除，commit 落库路径唯一化。
- `ChapterRepo::insert` **不删**（promotion.rs 生产调用方）。

---

## 4. 前端改动

### 4.1 types（`src/ipc/types.ts`）

- `ChapterSegment` 加 `title_line: number`。
- `ChapterInput` 加 `title_line: number`。
- `CommitDataAssetInput.chapters` 元素加 `title_line`。

### 4.2 store（`src/stores/chapters.ts`）

- 删：`markers` / `suppressed` / `segLineMap` / `segmentKey` / `addMarker` / `removeMarker` / `removeChapter` / `mergeSuppressed` / `applyTitleOverrides` / `computeLineMap` / `findSuppressedSegAtLine` / `startLineOf`，以及 `applyWorking` 里的 `splitChaptersByMarkers` / `computeLineMap` / `mergeSuppressed` 调用。
- 新增：`chapterSplits: Ref<Set<string>>`、`initialChapterSplits: Ref<Set<string>>`、`titles: Ref<Map<string, string>>`、`initialTitles: Ref<Map<string, string>>`、`toggleChapterSplit(key)`、`updateTitle(key, title)`、`recomputeInitialFromSegs(segs)`。
- `applyWorking` 改成 §2 纯函数（输入 chapterSplits + titles + rawLines，输出 workingChapters）。
- `load`：fresh 走 `list_chapter_segments`、committed 走 `list_committed_segments`，拿到 `[{title_line, title, content}]` 后 `chapterSplits = Set(title_line)`、`titles = Map(title_line → title)`、`initialChapterSplits = Set(...)`、`initialTitles = Map(...)`。`title_line` 越界（`<0` 或 `>= rawLines.length`）直接抛错。
- `toggleChapterSplit(key)`：对称 add/delete；add 时 `titles.set(key, stripInvisibles(rawLines[key]))`。
- `dirty` = 两 Set 不等 || 两 Map 不等。
- `reset`：`chapterSplits = new Set(initialChapterSplits)`、`titles = new Map(initialTitles)`。
- `commit`：`workingChapters` 映射 `{title_line, title, content}` 传 `commit_data_asset`；成功后重算 initial 快照。
- `reSplit`：调 `list_chapter_segments` 回到 splitter 划分，重算 initial。

### 4.3 splitChapters.ts

- 删：`splitChaptersByMarkers` / `diagnoseSplit` / `bodyStartByTitle` / `bodyStartByContent` / `parseChapterTitleStrict` / `parseChapterTitle` / `findHitsInRange`。
- 留：`isVisuallyEmptyLine` / `countChapterChars` / `INVIS_PREFIX_RE` / `INVIS_SUFFIX_RE` / `escapeRegExp`。
- 新增：`stripInvisibles(s)`（用 INVIS 正则 trim 首尾）、`stripTrailingInvisibles(s)`，给 store 的 applyWorking / toggleChapterSplit 复用。

### 4.4 parse.vue

- 删：merge `<Dialog>`、`pendingMerge`、`mergeDialogOpen`、`onMergeClick`、`cancelMerge`、`confirmMerge`，以及左侧「并入上一章」按钮。
- `markerSet` → `boundarySet`、`onMarkerToggle` → `onBoundaryToggle`（单行调 `store.toggleChapterSplit(key)`）。
- 章节跳转：`scrollToLine` 用 `workingChapters[i].title_line`（不再用 `startLineOf`）。
- 保存校验（`canCommit` / warn dialog）拆出到独立 spec，本 spec 不动保存按钮。

### 4.5 useParseEditor.ts

- `opts.markerSet` → `opts.boundarySet`、`syncMarkedClass` → `syncBoundaryClass`、`markerLineDeco` → `boundaryLineDeco`、`markerEffect` / `markerField` / `MarkerStamp` 内部命名重构。
- 对外行为 + CSS class 名（`cm-marker-stamp` / `cm-marker-line` / `cm-marker-gutter` / `cm-marker-stamp--marked`）不变。
- `lineMarker` 对空行不渲染按钮的 guard 保留。

---

## 5. fail-fast 契约

| 场景 | 行为 |
|---|---|
| load 时 `title_line < 0` 或 `>= rawLines.length` | 抛错，带 upload_id / title_line / rawLines.length |
| `list_committed_segments` 遇 `chapter.title_line == None` | 抛错，带 chapter_id / data_asset_id |
| `applyWorking` 遇 `titles` 缺 key | 抛错，带缺失的 lineKey |
| `toggleChapterSplit` 加边界时 rawLines 越界 | 抛错 |
| 空行 fallback 单行段落 content 空 | 确定性输出（word_count=0 章节可见），不静默丢弃 |

不写任何 `??` 静默回退、不写 `if 找不到 { 用另一种方式找 }`、不写「title_line 无效则跳过该章」。

---

## 6. 不做什么

- 不改 transform 侧章节消费方式（`Chapter.body` / `word_count` 语义不变）。
- 不改 CSS class 名。
- 不引入新依赖。
- 保存校验（0 章硬规则 / 单章超 3 万 / 超 1 万软提示）拆出到独立 spec。
- 不处理空行 fallback 单行段落的 content 退化（记录为已知边界）。

---

## 7. 测试

### 7.1 Rust（`crates/nsc-core/tests/splitter_new.rs` 规整）

- 正则章节 `title_line` 正确（含**前导空行**场景，验证用 `m.end()` 非 `m.start()`）。
- 空行 fallback 统一化：`title = 首行`、`content` 不含首行、`title_line` 指向首行。
- 空 input / 无匹配 input 行为不变。
- 删除 `split_with_edits` 相关旧用例。

### 7.2 前端 vitest

- `toggleChapterSplit`：body 行加边界、标题行删边界、重复 toggle 净变化 0、空栈合法。
- `applyWorking`：标题行边界 → content 从 key+1；改标题后 content 不变；`titles` 缺 key 抛错。
- `dirty`：toggle 后 true、改标题后 true、load 后 false、reset 后 false。
- `reset`：恢复 initialChapterSplits + initialTitles，清 titleOverrides。
- `commit`：传参 `{title_line, title, content}` 形状。
- 删除 `splitChaptersByMarkers` / `diagnoseSplit` 用例；保留 `isVisuallyEmptyLine` / `countChapterChars`。

### 7.3 e2e（playwright）

- body 行 toggle 章 → 左列表 +1、该行绿章。
- 章节标题行 toggle 章 → 该章消失、左列表 -1。
- 同一行 toggle 两次 → 净变化 0。
- reset → 恢复初始划分。

---

## 8. 自审清单

- [x] 无 TBD / TODO。
- [x] `title_line` 是唯一真相源，前端零反查、零双轨。
- [x] fail-fast 契约明确，无 `??` 兜底、无静默回退。
- [x] `title_line` 可空（NULL = promoted 无坐标），原始章节非 NULL，无迁移 backfill。
- [x] 死代码（`parse_chapters` / `replace_all_for_data_asset` / `split_with_edits` / 反查函数）删除；`insert` 保留（promotion.rs 生产调用方）。
- [x] `title_line` 用 `m.end()` 的 off-by 陷阱已点明，测试覆盖前导空行。
- [x] 保存校验拆出，scope 聚焦。
