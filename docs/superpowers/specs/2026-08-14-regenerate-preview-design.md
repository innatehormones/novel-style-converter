# 单章节重新生成预览 — 设计

**日期**：2026-08-14
**状态**：brainstorming 已收敛，待用户阅读 spec
**前置**：`docs/optimization-notes.md` "Workflow → DataAsset 转正" + `2026-08-04-workflow-results-design.md` §3.3 (wrc.content 唯一存储)
**关系**：不替代 `retry_empty_slots`；与之并存，分别覆盖不同场景

## 1. 目标

解决"工作流跑完后，用户对个别章节结果不满意想重做"这个高频场景。三个具体诉求：

1. **看到结果再决定**：新结果未确认前，原 `wrc.content` 不动；用户可以多次生成对比，挑一个最满意的再提交
2. **不污染工作流列表**：500 章小说若有 50 章效果不好，不应该新增 50 个 1 章工作流。重新生成与原工作流属于同一批次，复用现有 batch / tc 行
3. **支持微调**：每次生成允许附一小段"附加指令"（"再短一点"、"换个语气"），不必新建一个工作流改 prompt

## 2. 范围

### 2.1 包含

- 新表 `chapter_previews`（独立于 `workflow_result_chapters`）
- 单章节"重新生成预览"对话框（左：原文 tabs / 中：附加指令 / 右：预览 tabs）
- 多预览并存：关闭对话框不丢，下次进来仍能选择
- 提交 = 用选中预览覆盖 `workflow_result_chapters.content`，删除其他 preview 行
- 状态联动：`transformation_chapters.status = done`、tokens 更新、错误清空
- AI 调用走 recorder：`ai_call_logs.business = RegeneratePreview`
- 沿用工作流已固化的 ctx 设置（`ctx_prev_original` / `ctx_prev_transformed` / `ctx_next_original`），不允许在对话框内调整

### 2.2 不包含

- 批量多章节预览（一次预览多个章节）
- 预览过程中的 prompt 模板 / model 切换（沿用工作流设置）
- 预览结果版本历史 / diff（覆盖就是覆盖，不留快照）
- 自动定时重新生成
- 跨工作流预览迁移

## 3. 核心业务约束

### 3.1 不破坏现有转正契约

`workflow_result_chapters.content` 是转正（`promote_workflow`）时 `done` 章节 body 的**唯一来源**（`promotion.rs:107`）。预览提交 = 覆写这个唯一来源；覆写后已转正的 data_asset **不受影响**（转正时 body 已拷贝，不引用 wrc.content），新转正会用最新内容。

### 3.2 preview 与 wrc 的边界

| 维度 | workflow_result_chapters.content | chapter_previews.preview_content |
|---|---|---|
| 生命周期 | 跟随 batch 终止后保留 | 跟随提交动作删除（提交后清空） |
| 数量 | 每个 (batch_id, chapter_id) 至多一行 | 每个 (batch_id, chapter_id) 可多行（按时间倒序） |
| 提交语义 | 用户已认可的"成品" | 用户还没决定的"草稿" |
| 写入权限 | 仅 transformer on_chapter_done / preview 提交 | 仅 regenerate-preview AI 调用 |

不允许：preview 自动同步到 wrc（用户没点头之前一切都是草稿）；不允许：transformer 直接读 preview（preview 是草稿空间，不参与正常转换链路）

### 3.3 中间输入框语义

中间输入框的文本作为**附加指令**拼到 system prompt 后面，原 prompt 模板不变。规则：

- 用户输入为空 → 不拼接，与原转换行为完全一致
- 用户输入非空 → 在 system prompt 文末追加：

```
---

附加指令：
{user_input}
```

- 渲染走现有 `prompts::render` 流水线，不引入新模板变量
- 长度上限 2000 字（前端硬限制），避免 prompt 爆模型 context window

### 3.4 AI 调用业务类型

扩展 `AiCallBusiness` enum：

```rust
pub enum AiCallBusiness {
    TransformChapter,    // 已存在
    TestModel,           // 已存在
    RegeneratePreview,   // 新增
}
```

每次预览生成 = 一次 AI 调用 = 一条 `ai_call_logs`。`context_type = chapter_preview`，`context_id = chapter_previews.id`。这样"AI 调用"页面可以按业务筛选预览记录。

## 4. 数据模型

### 4.1 新表 chapter_previews

```sql
CREATE TABLE chapter_previews (
  id INTEGER PRIMARY KEY,
  batch_id INTEGER NOT NULL,
  chapter_id INTEGER NOT NULL,
  custom_input TEXT,                  -- 中间输入框内容（可空）
  preview_content TEXT,               -- AI 输出（NULL 表示生成中或失败）
  tokens_in INTEGER,
  tokens_out INTEGER,
  error TEXT,                         -- status=failed 时填错误消息
  status TEXT NOT NULL,               -- generating / done / failed
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (batch_id) REFERENCES batches(id) ON DELETE CASCADE,
  FOREIGN KEY (chapter_id) REFERENCES chapters(id) ON DELETE CASCADE
);
CREATE INDEX idx_chapter_previews_chap ON chapter_previews(batch_id, chapter_id, id DESC);
```

迁移编号：`0022_chapter_previews.sql`，追加到 `SCHEMAS` 数组。

### 4.2 状态机

```
用户点"生成"
  → INSERT chapter_previews (status='generating')
  → recorder 异步调 AI
    → 成功: UPDATE preview_content + tokens, status='done'
    → 失败: UPDATE error + status='failed'

用户点"确认此预览" (preview X)
  → BEGIN
    UPDATE workflow_result_chapters SET content = X.preview_content WHERE (batch_id, chapter_id) = (...)
    UPDATE transformation_chapters SET status='done', error=NULL, result_content=NULL, tokens_in=X.tokens_in, tokens_out=X.tokens_out, completed_at=now WHERE id=(...)
    DELETE FROM chapter_previews WHERE (batch_id, chapter_id) = (...) AND id != X.id
    -- X 自己这一行也删（提交后无意义）
    DELETE FROM chapter_previews WHERE id = X.id
  → COMMIT
```

### 4.3 不做版本快照

不引入 `workflow_result_chapter_versions` 表。提交前原内容在 `wrc.content` 原位不动，提交后立即被覆写。如果用户后悔，唯一的"恢复"方式是再生成一次新预览（不能回到旧版本）。这是用户决定的设计选择：避免冗余存储 + UI 复杂度。

## 5. API 表面

### 5.1 Rust 方法（`BatchScheduler`）

```rust
/// 异步：发起一次预览生成。返回新插入的 preview id。
/// AI 调用同 transformer.transform 路径（prompt + ctx + custom_input 拼接），
/// 完成后回调：UPDATE preview row + enqueue dispatcher（无需 advance_batch）。
pub async fn regenerate_preview(
    &self,
    batch_id: i64,
    chapter_id: i64,
    custom_input: Option<String>,
) -> Result<i64>; // 返回 preview.id

/// 同步：用指定 preview 覆盖 wrc.content，并清理同章节其他 preview。
pub fn commit_preview(
    &self,
    preview_id: i64,
) -> Result<WorkflowSummary>;

/// 同步：列出某章节的所有 preview（按 id DESC）。
pub fn list_chapter_previews(
    &self,
    batch_id: i64,
    chapter_id: i64,
) -> Result<Vec<ChapterPreviewRow>>;

/// 同步：放弃（删除）某个 preview 行。
pub fn discard_preview(
    &self,
    preview_id: i64,
) -> Result<()>;
```

### 5.2 Tauri command

```rust
#[tauri::command]
pub async fn regenerate_chapter_preview(
    db: State<'_, Arc<Mutex<Db>>>,
    scheduler: State<'_, Arc<BatchScheduler>>,
    batch_id: i64,
    chapter_id: i64,
    custom_input: Option<String>,
) -> Result<i64, String>;

#[tauri::command]
pub fn commit_chapter_preview(
    db: State<'_, Arc<Mutex<Db>>>,
    scheduler: State<'_, Arc<BatchScheduler>>,
    preview_id: i64,
) -> Result<WorkflowSummary, String>;

#[tauri::command]
pub fn list_chapter_previews(
    db: State<'_, Arc<Mutex<Db>>>,
    batch_id: i64,
    chapter_id: i64,
) -> Result<Vec<ChapterPreviewRow>, String>;

#[tauri::command]
pub fn discard_chapter_preview(
    db: State<'_, Arc<Mutex<Db>>>,
    scheduler: State<'_, Arc<BatchScheduler>>,
    preview_id: i64,
) -> Result<(), String>;
```

注册到 `src-tauri/src/commands/workflows.rs`，handler 在 `src-tauri/src/lib.rs` invoke_handler。

### 5.3 类型（IPC）

```rust
#[derive(Debug, Serialize)]
pub struct ChapterPreviewRow {
    pub id: i64,
    pub batch_id: i64,
    pub chapter_id: i64,
    pub custom_input: Option<String>,
    pub preview_content: Option<String>,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
    pub error: Option<String>,
    pub status: PreviewStatus,  // generating / done / failed
    pub created_at: String,
    pub updated_at: String,
}
```

前端 `ChapterPreviewRow` TS interface 同步加在 `src/ipc/types.ts`。

### 5.4 Store / Pinia

`src/stores/workflows.ts` 加：
- `regeneratePreview(batchId, chapterId, customInput): Promise<number>`
- `commitPreview(previewId): Promise<WorkflowSummary>`
- `loadPreviews(batchId, chapterId): Promise<ChapterPreviewRow[]>`
- `discardPreview(previewId): Promise<void>`

复用现有轮询基础设施：preview 生成中时，沿用 workflow 的 2 秒轮询节奏把 `chapter_previews` 列表拉回来即可，不需要新轮询通道。

## 6. UI 设计

### 6.1 触发入口

"工作流详情 modal" 的章节列表 cell-actions 现有"重新转换"按钮（`reconvertSingle` → `store.retry`）拆分为：

- 当前 status = done / failed / skipped（任意非空状态） → 改成"重新生成"按钮，点击打开预览对话框
- 当前 status = running / pending → 按钮置灰

旧的 `store.retry()` 链路保留，只服务于"failed/skipped 空槽复重试"（补漏），走原 `retry_empty_slots` 路径，无预览。

### 6.2 对话框布局

```
┌─────────────────────────────────────────────────────────────────┐
│ 重新生成章节 #X                                  [×]            │
├─────────────────────┬───────────────┬───────────────────────────┤
│ 原文 (3 tabs)       │ 附加指令      │ 预览 (N tabs)             │
│                     │               │                           │
│ [上一章] [当前] [下一章] │ ┌─────────┐ │ [预览 1 12:34] [预览 2 12:36] [+]│
│                     │ │         │ │                           │
│ ┌─────────────────┐ │ │         │ │ ┌───────────────────────┐ │
│ │                 │ │ │  textarea│ │ │                       │ │
│ │   章节正文     │ │ │  0/2000  │ │ │   预览正文          │ │
│ │                 │ │ │         │ │ │                       │ │
│ └─────────────────┘ │ │         │ │ └───────────────────────┘ │
│                     │ └─────────┘ │                           │
│                     │ [生成]      │ [确认此预览] [放弃]        │
└─────────────────────┴───────────────┴───────────────────────────┘
```

- 左栏：tabs = 上一章 / 当前章 / 下一章；不存在则不显示对应 tab（如 idx=1 无上一章）。tabs 间互斥切换，正文占左侧 1/3 宽度
- 中栏：textarea（高度约 12 行），空时不拼接指令。`[生成]` 按钮：textarea 空时禁用，提示"附加指令为空将按原 prompt 生成"
- 右栏：tabs 按 preview id DESC 排列。`[+]` = 触发中栏 `[生成]`。选中 tab 显示正文 + 元数据（tokens / 生成时间 / 自定义输入）
- 提交按钮 = 强确认 modal：

```
确认替换章节 #X 的结果？
预览 #N（生成于 HH:MM:SS）
[预览文本 100 字截断...]

⚠ 此操作：
  · 用此预览替换原 wrc.content（不可恢复）
  · 删除此章节下其他 N-1 条预览
  · tc 行 status → done，tokens 更新

[取消]  [确认替换]
```

### 6.3 状态联动

- preview `status=generating` 时，右栏对应 tab 显示转圈 + "生成中..." + 实时更新
- preview `status=failed` 时，tab 显示错误信息，禁用"确认此预览"按钮
- 用户关闭对话框：preview 行保留在 DB，下次打开对话框时通过 `list_chapter_previews` 拉回

### 6.4 边界情况

| 情况 | 行为 |
|---|---|
| workflow 已 promote，但用户想重新生成某章节 | 允许。提交预览后仅替换 wrc.content，已 promote 的 da body 不变 |
| 用户连续点 3 次"生成" | 创建 3 条 preview 行（并发）；右栏 3 个 tab；最先生成的不会"被覆盖" |
| preview 生成中（status=generating）用户关闭对话框 | 行保留，下次打开继续显示"生成中"。如不再关心，可点 tab 上的 [放弃] 删除 |
| 用户点"放弃"删除最后一个 preview | 允许。删除后右栏显示空状态"尚未生成预览" |
| 用户切到其他小说再切回来 | preview 按需懒加载，不缓存 |

## 7. 测试

### 7.1 单元测试

- `regenerate_chapter_preview` 的 ctx 拼接正确性（custom_input 为空时与原转换 byte-equal）
- `commit_preview` 单事务原子性（commit 失败时其他 preview 保留）
- `commit_preview` 后该章节所有 preview 行被删除（不留垃圾）
- `commit_preview` 不触发 `advance_batch`（preview 提交 ≠ 新工作流启动）
- `commit_preview` 不修改 batch 状态（wrc.content 覆写 ≠ batch 终止信号）
- preview 状态机迁移正确（generating → done / failed）

### 7.2 集成测试

- 完整链路：done 章节 → 预览生成 → commit → wrc.content 变化 → 转正行为验证
- 边界：preview 生成中调用 `discard_preview`（竞态）
- 边界：custom_input 含特殊字符（`\n` / 多行 / 2000 字上限）

## 8. 实施步骤（建议）

1. migration `0022_chapter_previews.sql` + `SCHEMAS` 数组追加
2. `AiCallBusiness` enum 加 `RegeneratePreview` 变体
3. `BatchScheduler::regenerate_preview` / `commit_preview` / `list_chapter_previews` / `discard_preview`
4. Tauri command 4 个 + `ChapterPreviewRow` 类型 + `invoke_handler` 注册
5. 前端 `ChapterPreviewRow` TS + Pinia store 4 个方法
6. 前端 `RegeneratePreviewDialog.vue` 组件
7. `TransformationNovelDetail.vue` cell-actions 替换"重新转换" → "重新生成" 按钮 + 打开对话框
8. 强确认 modal 复用现有 Dialog 组件
9. 轮询：现有工作流章节轮询节奏顺便拉 preview 列表（`list_chapter_previews` 在同一次 IPC 调用里合并）
10. ai_call_logs 页面加 `RegeneratePreview` 业务筛选

## 9. 未决项

无。设计已收敛到本 spec 描述的状态。

## 10. 与既有 spec 的关系

- **不替代** `retry_empty_slots`：补漏场景（failed/skipped 空槽）仍走原路径，无预览，无 workflow 列表污染
- **不破坏** `promote_workflow`：wrc.content 覆写后，已 promote 的 da body 仍指向旧值（转正时已拷贝），新 promote 用新内容
- **依赖** `2026-08-04-workflow-results-design.md` §3.3：wrc.content 是 done 章节正文的唯一来源
- **依赖** `2026-08-12-promote-workflow-to-data-asset-design.md`：确认 promoted da 的 body 拷贝语义
