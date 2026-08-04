# 工作流独立结果集设计

**日期**：2026-08-04  
**状态**：已完成 brainstorming，待用户审阅书面 spec  
**替代范围**：替代 `2026-08-03-transform-redesign-design.md` 中“已转换章节不可再勾选”、跨 batch frontier、`on_failure_policy`、多状态 batch，以及 `transformation_chapters` 同时承担任务与结果的约定。

## 1. 目标

把转换工程明确拆成三个层次：

1. `data_asset` 与源章节是不可变蓝本；
2. 工作流负责一次所选章节的执行过程；
3. 每个工作流拥有一个独立结果集，是一个潜在的新数据资产。

同一源章节可以同时加入任意多个工作流，不锁定、不警告、不限制转换次数。不同工作流的结果和上下文完全隔离。

## 2. 当前范围

### 2.1 包含

- 任务与结果数据拆分；
- 工作流 `Running / Stopped` 两态生命周期；
- 章节勾选、新建工作流、人工停止、停止后重试；
- 同一源章节的多工作流结果查看；
- 当前工作流内的串行调度与上下文继承；
- 旧 batch 结果数据迁移；
- 启动时安全恢复中断工作流。

### 2.2 不包含

- 结果集转正为正式 `data_asset`；
- 转正时补齐未选章节；
- 合并不同工作流的结果；
- 同一任务的多次 attempt 历史；
- 强制取消正在进行的 LLM HTTP 请求；
- 整本自动转换模式。

## 3. 核心业务约束

### 3.1 不可变蓝本

- `data_assets` 与 `chapters` 只提供源内容。
- 工作流只读源章节，转换成功后不得回填或修改源数据资产。
- 源章节不因其他工作流处于 `Running` 而被占用。

### 3.2 工作流与结果集

- 一次创建操作产生一个工作流。
- 一个工作流只对应一个结果集。
- 结果集只包含本次所选章节，不自动混入未选章节原文或其他工作流结果。
- 每个所选章节在该结果集中只有一个结果槽。
- 同一章节在不同工作流中拥有不同结果槽，历史互不覆盖。
- 失败或跳过时结果槽保留，内容为 `NULL`；重试成功后填充原槽。

### 3.3 生命周期

工作流生命周期只有：

```text
Running → Stopped
Stopped → Running   // 用户重试空结果槽
```

- 全部任务处理完后自动 `Stopped`。
- `Failed / Skipped` 是章节任务结果，不是工作流生命周期状态。
- 工作流列表可展示 Done、Failed、Skipped 的派生数量，但不得把它们写成工作流状态。
- `Stopped` 结果集未来都具有转正资格，即使存在空结果槽；转正规则不在本次范围。

## 4. 数据模型

```text
DataAsset
└─ Chapter

TransformationNovel
└─ batches                         // 内部表名保留，业务语义为 Workflow
   ├─ transformation_chapters      // 执行任务
   └─ workflow_results             // 1:1 独立结果集
      └─ workflow_result_chapters  // 所选章节的结果槽
```

### 4.1 `batches`：工作流

保留现有表，修改领域语义：

- `status`：新数据只写 `running | stopped`；
- `label`：工作流标签；
- `started_at`：首次开始时间；
- `ended_at`：最近一次停止时间；重试开始时清空；
- `on_failure_policy`：保留旧列以兼容已应用 migration，新流程不读取、不写入业务设置。

前端和业务文案统一使用“工作流”，不向用户暴露“batch 状态机”。

### 4.2 `transformation_chapters`：执行任务

现有表改为纯任务记录：

- 任务状态使用 `pending | running | done | failed | skipped`；
- `cancelled` 仅作为存量兼容值，迁移时映射为 `skipped`；
- 保存 prompt、model、mode、上下文数量、token、错误和执行时间；
- 新流程不再把 `result_content` 作为结果读取来源；
- 同一非空 `batch_id` 下，同一 `chapter_id` 只能有一条任务。

`Failed` 后任务保留错误，`Skipped` 可记录人工停止原因。重试复用该任务行，清空错误和执行时间后回到 `Pending`。

### 4.3 `workflow_results`：结果集

新表：

```sql
CREATE TABLE IF NOT EXISTS workflow_results (
  id         INTEGER PRIMARY KEY,
  batch_id   INTEGER NOT NULL UNIQUE REFERENCES batches(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL
);
```

一个工作流在创建时同步创建一条结果集记录。

### 4.4 `workflow_result_chapters`：结果槽

新表：

```sql
CREATE TABLE IF NOT EXISTS workflow_result_chapters (
  id                 INTEGER PRIMARY KEY,
  workflow_result_id INTEGER NOT NULL REFERENCES workflow_results(id) ON DELETE CASCADE,
  chapter_id         INTEGER NOT NULL REFERENCES chapters(id),
  content            TEXT,
  created_at         TEXT NOT NULL,
  updated_at         TEXT NOT NULL,
  UNIQUE(workflow_result_id, chapter_id)
);
```

任务与结果槽通过 `(batch_id, chapter_id)` 对应：`batch_id → workflow_results.id`，再按 `chapter_id` 找到唯一结果槽。结果槽不复制任务状态、错误或 token。

### 4.5 Migration 0011

新增 `0011_workflow_results.sql`，不修改已应用 migration：

1. `CREATE TABLE IF NOT EXISTS` 创建两张结果表；
2. 创建 `transformation_chapters(batch_id, chapter_id)` 的 partial unique index，`batch_id IS NOT NULL` 时生效；
3. 为每个存量 batch 建立一个结果集；
4. 为每条带 `batch_id` 的存量任务建立结果槽；仅 `status='done'` 时回填旧 `result_content`；
5. 存量 `cancelled` 任务映射为 `skipped`；
6. 存量 batch 统一安全归档为 `stopped`；
7. `batch_id IS NULL` 的历史散点转换保留只读，不强行归入工作流。

所有新增表和索引使用 `IF NOT EXISTS`，回填使用可重复执行的 `INSERT OR IGNORE`。

## 5. 创建与调度

### 5.1 原子创建

前端提交一个原子命令，不再执行“先 create_batch、再 dispatch_batch”的两步流程：

```text
create_workflow {
  tn_id,
  label,
  chapter_ids[],
  prompt_id,
  model_config_id,
  mode,
  ctx_prev_original,
  ctx_prev_transformed,
  ctx_next_original
}
```

后端在同一事务内：

1. 验证至少选择一个章节；
2. 验证所有章节属于该转换工程关联的数据资产；
3. 验证 prompt、model 存在且 prompt kind 与 mode 一致；
4. 创建 `batches(status='running')`；
5. 创建一个 `workflow_results`；
6. 按所选章节创建 N 个 `Pending` 任务；
7. 创建 N 个空结果槽；
8. 提交事务后派发章节序号最小的任务；若队列不可用，则把任务安全归档为 `Skipped`、工作流改为 `Stopped`，并向前端返回错误。

任一步数据库写入失败都不得留下孤立工作流、任务或结果槽。

### 5.2 串行处理

同一工作流任意时刻最多一个任务 `Running`，不同工作流可并行：

- 成功：在一个事务中把内容写入结果槽，并把任务改为 `Done`；
- 失败：任务改为 `Failed`，错误保留，结果槽内容保持 `NULL`；
- 成功或失败后都继续派发当前工作流下一个 `Pending` 任务；
- 不存在 `Pending / Running` 任务时，工作流改为 `Stopped` 并写 `ended_at`。

新流程删除 `pause_and_review / terminate / skip_failed` 三种失败策略。失败固定采用“标记失败并继续”。

### 5.3 当前工作流内上下文

“前文转换”只读取当前工作流结果集：

```sql
SELECT wrc.content
  FROM workflow_result_chapters wrc
  JOIN workflow_results wr ON wr.id = wrc.workflow_result_id
  JOIN chapters c ON c.id = wrc.chapter_id
 WHERE wr.batch_id = ?
   AND wrc.content IS NOT NULL
   AND c.idx < ?
 ORDER BY c.idx DESC
 LIMIT 1;
```

- 不跨工作流读取结果；
- 前一所选章节失败或跳过时，继续向前找当前工作流最近的非空结果；
- 当前工作流没有前序成功结果时，不注入“前文转换”；
- “前文原文 / 后文原文”仍从不可变源章节读取。

## 6. 人工停止与重试

### 6.1 人工停止

用户点击停止后必须二次确认。

后端停止命令在事务中：

1. 若工作流已是 `Stopped`，幂等返回当前状态；否则继续；
2. 把所有 `Pending` 任务改为 `Skipped`，结果槽保持空；
3. 不强杀当前 `Running` 任务；
4. 若当前没有 `Running` 任务，立即把工作流改为 `Stopped`；
5. 若当前任务仍在执行，等待其成功或失败落库；回调发现已无 `Pending` 后把工作流改为 `Stopped`。

不需要增加第三种生命周期状态。停止命令与 worker 回调必须在事务中重新读取任务状态，避免继续派发。

### 6.2 Stopped 后重试

- 只有 `Stopped` 工作流可发起重试；
- 只能选择结果内容为空且任务为 `Failed / Skipped` 的章节；
- 可一次选择一个或多个空槽；
- 复用原任务和原结果槽，不产生新结果版本；
- 任务重置为 `Pending`，清空错误和执行时间；
- 工作流改为 `Running`、清空 `ended_at`；
- 按章节序号串行执行，结束后再次 `Stopped`。

## 7. 启动恢复

应用启动时执行安全恢复：

- 数据库中遗留的 `Running` 任务改为 `Failed`，错误说明为进程中断；
- 同工作流尚未执行的 `Pending` 任务改为 `Skipped`；
- 遗留 `Running` 工作流改为 `Stopped`；
- 不自动重新调用模型。

用户进入工作流详情后，可检查空结果槽并主动重试。

## 8. IPC 与查询边界

新增或重塑以下业务命令：

- `list_transformation_source_chapters(tn_id)`：返回源章节一览及非空结果数量；
- `create_workflow(payload)`：原子创建并运行工作流；
- `list_workflows(tn_id)`：返回工作流与任务数量汇总；
- `get_workflow(batch_id)`：返回工作流详情；
- `list_workflow_chapters(batch_id)`：返回源章节、任务和结果槽的 join 行；
- `stop_workflow(batch_id)`：人工停止；
- `retry_workflow_chapters(batch_id, chapter_ids)`：Stopped 后重试空槽；
- `list_chapter_workflow_results(tn_id, chapter_id)`：按工作流列出该源章节的多份结果。

Tauri 外层参数保持 camelCase，内层 DTO 保持 snake_case。旧 batch IPC 可在前端迁移完成后删除，不保留双路径兼容层。

## 9. 前端交互

### 9.1 章节一览

- 查询源章节，每章固定一行；
- 列：勾选、序号、标题、字数、已有结果数；
- 所有章节始终可选，默认全选；
- 提供“全选 / 全不选 / 反选 / 新建工作流（N 章）”；
- 点击标题查看源原文和按工作流区分的多份结果；
- “已有结果数”只统计 `content IS NOT NULL` 的结果槽。

### 9.2 新建工作流弹窗

显示：

- 已选 N 章；
- prompt；
- model；
- label；
- 前文原文、前文转换、后文原文数量。

不在弹窗中再次选择章节，不显示失败策略。`mode` 从 prompt kind 推导，后端再次校验。

创建成功后切换到“工作流”tab并打开新工作流详情。

### 9.3 工作流 tab

列表展示：

- label；
- `Running / Stopped`；
- 总章节数；
- Done / Failed / Skipped 数；
- 创建时间与停止时间。

详情展示章节序号、标题、任务结果、结果预览和错误：

- Running：显示“停止工作流”，点击后二次确认；
- Stopped：允许勾选结果为空的 Failed / Skipped 行并“重试所选”；
- 当前阶段不展示“转正为数据资产”。

## 10. 错误处理

| 场景 | 行为 |
|---|---|
| 章节选择为空 | 前端禁用提交，后端仍返回 Validation |
| 章节不属于工程数据资产 | 整个创建事务回滚 |
| prompt / model 无效 | 整个创建事务回滚 |
| LLM 请求失败 | 任务 Failed、结果为空、继续下一章 |
| 结果写入事务失败 | 不允许任务变 Done；保留可恢复状态和错误 |
| Running 时请求重试 | Validation：必须先停止 |
| 重试非空结果槽 | Validation：已有结果不可在原工作流覆盖 |
| 重复停止 | 返回当前 Stopped 状态，不再次修改任务 |
| 应用中断 | 下次启动安全停止，不自动重放请求 |

## 11. 测试策略

### 11.1 Rust

- migration 0011 可重复执行；
- 旧 Done 内容正确回填，Failed / Skipped 槽为空；
- 同工作流同章节唯一，不同工作流可重复同一章节；
- 创建事务任一步失败均无孤立数据；
- 批内串行、批间并行；
- 失败后继续、自然停止；
- 人工停止不强杀当前章，后续任务 Skipped；
- Stopped 后多章重试复用原槽；
- Running 时禁止重试；
- frontier 只读取当前工作流；
- 启动安全恢复。

### 11.2 Frontend

- 源章节默认全选；
- 全选、全不选、反选；
- 同章已有或正在运行其他工作流时仍可选择；
- 弹窗显示所选数量并提交精确 payload；
- mode 从 prompt kind 推导；
- 停止前二次确认；
- 只有 Stopped 空槽可勾选重试；
- IPC camelCase 外层参数与 snake_case DTO 精确匹配。

### 11.3 手工验证

启动 Tauri/Vite 实际验证：

1. 同一章节同时加入两个工作流并得到两份独立结果；
2. 一个工作流内失败后继续下一章；
3. 人工停止时当前章完成、后续章 Skipped；
4. Stopped 后重试空槽；
5. 章节详情按工作流展示多份结果；
6. 源数据资产内容始终未改变。

## 12. 完成判据

- 源章节无转换次数和并发选择限制；
- 工作流只有 Running / Stopped 两种生命周期状态；
- 每个工作流有且仅有一个独立结果集；
- 任务状态与结果内容分表保存；
- 失败继续、自动停止、人工停止和停止后重试符合本 spec；
- 上下文不跨工作流；
- 新旧数据迁移与所有自动化测试通过；
- 前端黄金路径完成真实交互验证；
- 不实现结果集转正或跨工作流结果合并。
