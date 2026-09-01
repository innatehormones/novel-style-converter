# 「新建工作流」试运行区 — 设计

**日期**：2026-08-20
**状态**：brainstorming 已收敛，待用户阅读 spec
**前置**：
- `docs/superpowers/specs/2026-08-04-workflow-results-design.md`（workflow_results / workflow_result_chapters schema）
- `docs/superpowers/specs/2026-08-14-regenerate-preview-design.md`（单章节重新生成，预览相关的 ai_call_logs 业务类型）
- `crates/nsc-core/src/transformer/transformer.rs` 的 `transform_with_business(req, business)` 路径

**关系**：
- 不替代「单章节重新生成」（那是工作流跑完后调整某章），这次是「工作流**创建前**预览首章」
- 后端 `create_workflow` 扩展一个入参，不另起一套

## 1. 目标

解决两个高频痛点：

1. **首章无法预览**：用户在「新建工作流」对话框选好 prompt / 模型 / ctx 后，看不到首章会变什么样。点了「创建」就开 batch，第一章 dispatch 后才出结果；如果首章的前文来源（数据资产 / 上一章原文）有问题，AI 带着错前文继续生成后续章节，浪费 token 还无法阻止。
2. **prompt 调试链路长**：要新建工作流、跑第一章才能看到 prompt 模板的实际效果，反馈环太长。

**本次范围只覆盖第一个痛点**（工作流创建时的试运行区）。第二个痛点（prompt 编辑器内的预览）记 TODO，本次不做。

## 2. 范围

### 2.1 包含

- 「新建工作流」对话框改左右分栏布局
- 右侧上半：context 简化区（两个 toggle：带前文 / 带后文）
- 右侧下半：试运行区（首章原文 + 生成按钮 + 预览结果 + 重新生成 / 使用此结果）
- 新增 IPC 命令 `preview_first_chapter`：调一次 `transform_with_business(req, RegeneratePreview)`，返回 `{ content, tokens_in, tokens_out }`
- `create_workflow` 入参扩展 `preview_first_chapter`，事务内把 idx=0 的 tc 标 done + 写 result_content；batch 从 idx=1 开始自然推进
- ai_call_logs 每次预览写一条记录（`business = RegeneratePreview`，复用现有枚举）

### 2.2 不包含

- prompt 编辑器内的预览（TODO）
- 预览章节的切换（下拉切换 idx）—— 本次固定为 idx=0（章节范围最小 idx）；多 idx 预览留 TODO
- 预览结果的历史版本 / diff
- 自动定时预览
- 跨工作流预览迁移

## 3. 核心业务约束

### 3.1 预览产物落库语义

用户在右侧反复点「生成预览」/「重新生成」，结果**只存在前端 dialog 状态**（不落库）。满意后点「创建」：

- 前端把 `preview_first_chapter: { content, tokens_in, tokens_out }` 作为入参传给 `create_workflow`
- 后端在 `create_workflow` 事务内：
  1. 创建 batch（status=pending）
  2. 创建所有 selectedChapterIds 对应的 tc（status=pending）
  3. **额外**：把 idx 最小那个 chapter 对应的 tc 的 `status='done'`、`result_content=content`、`tokens_in/out`、`completed_at=now`
  4. workflow_results + workflow_result_chapters 也写 idx 最小那个 chapter 的内容
  5. dispatch：scheduler 找到 idx 最小那个 chapter 对应的 tc（已 done）后跳过，向后找下一个 pending tc（idx 最小+1 或下一个存在的）派发

效果：
- 用户视角：手动生成满意的首章 → 创建工作流 → batch 自动从 idx=1 开始跑
- 数据视角：idx=0 跟正常 done 无差别，下游（promote_workflow / AI 调用日志）无需特殊处理

### 3.2 ctx 简化：从数值到 toggle

当前：`ctx_prev_original` / `ctx_prev_transformed` / `ctx_next_original` 三个 i32 字段（NumberInput，max=20）

新设计：两个 boolean toggle：
- 「带前文」= true → `ctx_prev_original=1`, `ctx_prev_transformed=1`
- 「带前文」= false → `0`, `0`
- 「带后文」= true → `ctx_next_original=1`
- 「带后文」= false → `0`

理由：用户吐槽「填数值填多了没意义，AI 看不懂对比」「前文原文 + 前文转换是一体的，应该一起带」。本次简化，未来如需 N 章前文，再扩为高级模式。

后端 schema 不变（仍 i32），前端传 1/0。

### 3.3 预览不污染工作流状态

- 预览期间（前端反复「生成预览」）：**不创建** batch / tc 行；调 `preview_first_chapter` 单次跑 AI，写一条 ai_call_logs（business=RegeneratePreview），结果回前端展示
- preview_first_chapter 命令**不**触发 batch_scheduler 任何动作
- 用户中途关掉 dialog：preview 结果丢弃，无任何副作用

### 3.4 preview_first_chapter 与 transformer 路径

走现有 `DefaultTransformer::transform_with_business(req, AiCallBusiness::RegeneratePreview)`，复用：
- prompt 模板渲染（render）
- close_thinking 注入
- custom_input 拼接（spec §3.3 regenerate-preview 设计）
- max_context 估算校验
- recorder 写 ai_call_logs

TransformRequest 组装由**后端**完成，前端只传简单字段：
- `tn_id`: 转换工程 id（用于定位 data_asset_id + mode + selectedChapterIds 范围外的章节作前文）
- `chapter_id`: 要预览的章节 id（默认 = selectedChapterIds 中 idx 最小的那个）
- `prompt_id` / `model_config_id`
- `include_prev: bool` / `include_next: bool`
- `custom_input: Option<String>`（预留，本期 UI 不暴露，留给后续「附加指令」扩展）

后端根据 include_prev / include_next 计算实际的 prev_original / prev_transformed / next_original 字符串：
- prev_original = "idx < 当前 chapter.idx 的最近 N 章原文"（N=1）
- prev_transformed = "无"（idx=0 没有前文转换结果；idx>0 时是工作流已转换的最近 N 章）
- next_original = "idx > 当前 chapter.idx 的最近 N 章原文"（N=1）

注：idx=0 时 prev_* 都是 "(暂无原文/转换参考)"，符合现有 prompt 模板渲染。

## 4. 数据模型

### 4.1 不新增表

preview 结果**不落库**（仅 dialog 状态）。ai_call_logs 走现有表（business 枚举扩展）。

### 4.2 create_workflow 入参扩展

```rust
pub struct CreateWorkflowPayload {
    // ... 现有字段
    pub preview_first_chapter: Option<PreviewFirstChapter>,
}

pub struct PreviewFirstChapter {
    pub content: String,
    pub tokens_in: i32,
    pub tokens_out: i32,
}
```

`BatchScheduler::create_workflow` 入参 `WorkflowCreate` 加同样字段（`Option<PreviewFirstChapter>`）。

### 4.3 transformation_chapters 已有字段足够

`status` / `result_content` / `tokens_in` / `tokens_out` / `completed_at` 已在 schema 中，事务内直接 UPDATE 即可。

## 5. API 表面

### 5.1 新增 IPC 命令

```rust
#[tauri::command]
pub async fn preview_first_chapter(
    db: State<'_, Arc<Db>>,
    input: PreviewFirstChapterInput,
) -> Result<PreviewFirstChapterOutput, String>;

pub struct PreviewFirstChapterInput {
    pub tn_id: i64,
    pub chapter_id: i64,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub include_prev: bool,
    pub include_next: bool,
    pub custom_input: Option<String>,
}

pub struct PreviewFirstChapterOutput {
    pub content: String,
    pub tokens_in: i32,
    pub tokens_out: i32,
}
```

实现路径：
1. 读 chapter + chapter_content
2. 读 prompt + model_config（确保未归档）
3. 组装 PrevContext / NextContext 字符串
4. 调 `DefaultTransformer::transform_with_business(req, AiCallBusiness::RegeneratePreview)`
5. 返回 content + tokens

### 5.2 create_workflow 命令扩展

`CreateWorkflowPayload` 加 `preview_first_chapter: Option<PreviewFirstChapter>` 字段（IPC 边界透传）。`BatchScheduler::create_workflow` 接收 Option：
- None → 行为不变（所有 tc pending）
- Some → 在 INSERT tc 后，UPDATE idx 最小那个 chapter 对应的 tc 为 done + 写内容

### 5.3 IPC wrapper

`src/ipc/commands.ts` 新增 `previewFirstChapter(input)` + 类型定义。

## 6. UI 流程

### 6.1 CreateBatchDialog.vue 改版

- Dialog width: 540 → 880
- 内部 grid 2 列
- **左列**：标签 / Prompt / 模型 / 失败策略 / 章节选择（提示「已选 N 章」保留）
- **右列上半**：context 区
  - 「带前文」toggle（前文原文 + 前文转换一体）
  - 「带后文」toggle
  - 移除 ctx-hint（不再有「带 1~3 章」之类的提示）
- **右列下半**：试运行区
  - 章节元信息条：「预览章节 #X / 标题 / 字数」
  - 「原文」只读 textarea（max-height + scroll）
  - 「生成预览」按钮（loading 状态）
  - 生成成功后：
    - 「转换预览」只读 textarea
    - 「重新生成」+「满意，使用此结果」按钮
  - 「满意」按钮把结果缓存到 dialog 状态（preview_first_chapter 字段），按钮变「已选 ✓ 重新选」
  - 「已选 ✓ 重新选」状态下「创建」按钮 enabled（之前 disabled，提示「请先生成预览」）

### 6.2 章节范围与预览章节的对应

- selectedChapterIds 变化时，预览章节自动 = idx 最小的那个（不提供切换 UI）
- idx=0 没有前文：预览结果反映「空前文」效果，与 idx=1 实际跑时有前文转换的场景不一致——这是有意取舍
- 后续可扩：下拉切换 idx>0 作为预览章节（TODO）

### 6.3 错误处理

- 预览调用失败：按钮回到「生成预览」+ 错误信息显示；用户可重试
- 创建时 `create_workflow` 失败：dialog 不关闭，错误信息显示在底部，按钮恢复
- preview_first_chapter 为 null 时（用户没满意）：「创建」按钮 disabled + tooltip 提示

## 7. 测试策略

### 7.1 后端单测

- `BatchScheduler::create_workflow` 加 preview_first_chapter 路径测试：
  - 验证 idx 最小那个 chapter 对应的 tc 被标 done + result_content 写入
  - 验证 idx=1..N-1 tc 为 pending
  - 验证 advance_batch 派 idx=1
  - 验证 None 路径行为不变

### 7.2 后端命令测试

- `preview_first_chapter` 命令：
  - happy path：返回 content + tokens
  - chapter 不存在：NotFound 错误
  - prompt / model_config 归档：NotFound 错误
  - max_context 超限：Validation 错误（透传 transformer 行为）

### 7.3 前端手动测试

- 打开 dialog → 选 prompt / model → 点「生成预览」→ 看结果
- 反复「重新生成」5 次 → 看 ai_call_logs 多 5 条 RegeneratePreview
- 点「满意」→ 「创建」→ 工作流列表出现新 batch，idx=0 done，idx=1..N-1 pending → 等候跑完
- 关 dialog 不点创建：preview 结果丢弃，工作流列表无新增

## 8. TODO（不在本次范围）

- prompt 编辑器（Prompts.vue 编辑弹窗）内的轻量「测试」按钮，调用同一 preview_first_chapter 底层
- 试运行区支持切换预览章节（idx=1..N-1），让用户预览带前文后的效果
- ctx toggle 升级为「带前文」+ 「带前文 N 章」高级模式
