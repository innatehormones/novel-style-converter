# 转换工程工作流（batch + 串行 + frontier）— 设计 spec

**日期**:2026-08-03
**作者**:brainstorming 阶段产物
**状态**:已批准（待 writing-plans → 实施）
**关联 backlog**:压缩+文风组合 prompt、全本自动逐章转换、`auto_continue` 接力实现、章节一览 tab 内的"新建批量/勾选"交互
**关联旧文档**:本 spec 替代先前围绕"批号/frontier/serial/auto_continue"的非正式约定（无 doc 落盘）

---

## 1. 背景与约定（来自用户确认）

### 1.1 为什么"转换"

把已解析章节（数据资产）的正文交给 LLM 处理，常见两类目的：

- **文笔/文风调整** —— 让阅读体验更友好（Prompt.kind = `style`）
- **内容压缩** —— 让阅读更高效（Prompt.kind = `compress`）

本 spec 不引入第三类目的。

### 1.2 实体关系（强约束）

- 一个 **数据资产**（data_asset）→ **多个 转换工程**（transformation_novel）
- 转换工程互不耦合，每个工程有独立的"默认模型 + 默认 prompt + 默认 mode"
- 不在数据资产上加任何聚合字段

### 1.3 转换工程详情页 — 两 tab

#### 章节一览
- 列出该数据资产下所有章节，显示每章的转换状态
- 有转换的：点开看 原文 + 转换后文章（不做并排 diff）
- 无转换的：点开看原文
- **勾选未转换章节 → 批量转换 → 生成一个批号（一次批量 = 一个批号 = 一个工作流）**
- 批号内执行：串行、章节递增
- **上下文继承（frontier）**：转换第 N 章时，找前序最近一次已转换章节（按 chapter_idx 跨 batch 取），把它整段 result_content 塞进 prompt
- 每次批量可配置 **on_failure_policy**：pause_and_review / terminate / skip_failed
- **已转换章节不参与批量勾选**；若任一 `status='done'` 行存在 → 该章不可勾
- 用户主动的"风格参考章节"通过 prompt 中显式传入（见 §5.3）

#### 工作流
- 列批号；点开看批号内的章节进度/列表
- 展示能力与章节一览高度重复 —— 区别只在 **不可勾选**（只读进度）+ **可执行 resume**

### 1.4 不在本 spec（out of scope）
- 自动转换（整本逐章连续） —— `auto_continue` 列保留但不接逻辑
- 并排 diff 显示
- 压缩 + 文风组合 prompt（Prompt.kind 是单值）
- Tabs 内的多选交互、`batches.label` UI 展示

---

## 2. 范围与切片

### 2.1 In scope（一个 spec 完成）

- 数据库：`migration 0007`、3 张表/列改动、新 Rust 类型
- 后端：`BatchRepo` + 7 个 IPC 命令 + `BatchScheduler` 模块 + frontier/style_ref 计算
- 前端：`TnDialog` 增加 3 字段、`useBatchesStore`、路由 `/library/transformation/:tnId`、**只读**的两 tab 骨架（章节一览 + 工作流）

### 2.2 Out of scope（backlog / 下一 spec）

- 章节一览 tab 内的"勾选 + 新建批量"按钮（由用户在 paused 后改用 resume_batch 的工作流）
- 自动批号进度（动画条） —— 现只显示数字 N/M
- 章节详情内手动"重新转换"按钮
- `auto_continue` 行为实现
- 压缩 + 文风组合 prompt
- 全本自动逐章转换

---

## 3. 架构总览

### 3.1 模块树

```
crates/nsc-core/src/
├── db/
│   ├── repo/
│   │   ├── batch.rs                [NEW]  CRUD + status transitions
│   │   └── transformation.rs       [MOD]  helper: enqueue 时 stamp batch_id
│   ├── pool.rs                     [MOD]  Db 上挂 batches()
│   └── migrations/0007_*.sql       [NEW]
├── models/
│   ├── batch.rs                    [NEW]  Batch / NewBatch / BatchStatus / OnFailurePolicy
│   └── transformation.rs           [MOD]  TransformStatus 加 Skipped；TransformationChapter 加 batch_id / style_ref_chapter_id
└── transformer/
    ├── batch_scheduler.rs          [NEW]  核心调度
    └── job_queue.rs                [MOD]  enqueue 时通知 scheduler；on-finish callback 调用 scheduler

src-tauri/src/
├── commands/
│   └── batches.rs                  [NEW]  7 个 IPC 命令
└── lib.rs                          [MOD]  注册新命令 + 启动 BatchScheduler

src/
├── stores/
│   └── batches.ts                  [NEW]  useBatchesStore（极简 list by tn + resume）
├── ipc/
│   ├── commands.ts                 [MOD]  7 个 wrapper
│   └── types.ts                    [MOD]  Batch / BatchStatus / OnFailurePolicy / ResumeAction
├── components/
│   └── TransformationNovelDialog.vue  [MOD]  增 3 字段
├── router/index.ts                 [MOD]  /library/transformation/:tnId
└── views/
    ├── Library.vue                 [MOD]  transformations tab 加 "详情" 入口
    └── TransformationNovelDetail.vue  [NEW]  两 tab 骨架
```

### 3.2 不变量（保持现有项目规则）

- `Db` 仍 Send 非 Sync —— scheduler 持 `db_path: PathBuf`，工厂内 `Db::open`
- JobQueue 仍 2 worker，上限 4（per `ModelConfig.concurrency` 字段预留）
- 所有 migration DDL 仍 `IF NOT EXISTS`
- IPC 外层 camelCase（`tnId` / `batchId` / `onFailurePolicy`）/ 内层 DTO snake_case（`on_failure_policy`）

---

## 4. 数据模型（migration 0007）

### 4.1 SQL

```sql
-- tn 增 3 列（NULL 兼容存量）
ALTER TABLE transformation_novels
  ADD COLUMN default_model_config_id INTEGER REFERENCES model_configs(id),
  ADD COLUMN default_prompt_id       INTEGER REFERENCES prompts(id),
  ADD COLUMN default_mode            TEXT;  -- 'compress' | 'style'

-- batches 新表
CREATE TABLE IF NOT EXISTS batches (
  id                    INTEGER PRIMARY KEY,
  transformation_novel_id INTEGER NOT NULL REFERENCES transformation_novels(id),
  label                 TEXT,
  on_failure_policy     TEXT NOT NULL DEFAULT 'pause_and_review',
  status                TEXT NOT NULL DEFAULT 'pending',
  created_at            TEXT NOT NULL,
  started_at            TEXT,
  ended_at              TEXT
);
CREATE INDEX IF NOT EXISTS idx_batches_tn      ON batches(transformation_novel_id);
CREATE INDEX IF NOT EXISTS idx_batches_status  ON batches(status);

-- transformation_chapters 增 2 列（NULL 兼容存量）
ALTER TABLE transformation_chapters
  ADD COLUMN batch_id             INTEGER REFERENCES batches(id),
  ADD COLUMN style_ref_chapter_id INTEGER REFERENCES chapters(id);
CREATE INDEX IF NOT EXISTS idx_tc_batch ON transformation_chapters(batch_id);

-- TransformStatus 取值扩展为 'skipped'；DB 列是 TEXT 无 schema 改动
```

### 4.2 新 Rust 类型（`models/batch.rs`）

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus { Pending, Running, Paused, Completed, Terminated, Cancelled }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnFailurePolicy { PauseAndReview, Terminate, SkipFailed }

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
```

### 4.3 `models/transformation.rs` 改动

```rust
pub enum TransformStatus { Pending, Running, Done, Failed, Cancelled, Skipped }  // +Skipped

pub struct TransformationChapter {
    // 既有 14 列保留
    pub batch_id: Option<i64>,             // +NEW
    pub style_ref_chapter_id: Option<i64>, // +NEW
}
```

### 4.4 兼容性策略

- 存量 `transformation_chapters.batch_id = NULL` → UI 显示"未指定批号 / 历史散点"
- 存量 `transformation_novels` 新 3 列 NULL → 旧 tn 仍可用；新批量转换时强制要求 model + prompt + mode
- `batches` 表新建（无迁移负担）

---

## 5. BatchScheduler（核心模块）

### 5.1 模块位置与职责

`crates/nsc-core/src/transformer/batch_scheduler.rs`。单例，与 `JobQueue` 同寿命，由 `src-tauri/src/lib.rs` 创建并持有。

职责：

- **批内串行**：同一 batch 任意时刻最多 1 章在 worker pool
- **批间并行**：不同时刻可有多个 batch 各有 1 章 in-flight（受 JobQueue 2 worker 池限制）
- **故障分流**：on_failure_policy 决定 failed 后续拍板
- **frontier + style_ref**：在 enqueue 前算上下文字段塞进 prompt

### 5.2 公共 API

```rust
impl BatchScheduler {
    pub fn new(db_path: PathBuf, job_queue: Arc<JobQueue>) -> Self;

    /// 创建批号并立即入队若干章节（前端一次调用）
    pub fn create_batch(
        &self,
        new_batch: NewBatch,
        chapter_ids: Vec<i64>,
    ) -> Result<Batch>;

    /// 已存在 batch 追加章节（保留接口，本 spec 可不暴露给 UI）
    pub fn add_chapters(&self, batch_id: i64, chapter_ids: Vec<i64>) -> Result<()>;

    /// 由 JobQueue 完成回调
    fn on_chapter_done(&self, batch_id: i64, chapter_id: i64) -> Result<()>;
    fn on_chapter_failed(&self, batch_id: i64, chapter_id: i64, error: String) -> Result<()>;

    /// 用户操作（IPC 入口）
    pub fn resume(&self, batch_id: i64, action: ResumeAction) -> Result<Batch>;
}

pub enum ResumeAction {
    Retry(i64),     // 重试该章
    Skip(i64),      // 标记 skipped，继续
    Terminate,      // 终止整批
}
```

### 5.3 批号状态机

```
                    create_batch
                         │
                         ▼
                   ┌──────────┐
                   │ Pending  │
                   └────┬─────┘
                        │ (首章 dispatch)
                        ▼
             ┌──────────────────────┐
             │       Running        │ ◀────────────────┐
             └──┬────┬────┬────┬────┘                  │
                │    │    │    │                       │ resume(Retry|Skip)
       skip_failed │    │   │ pause_and_review          │
                  │    │    │   │                       │
                  │    │   │    ▼                       │
                  │   末章  │ ┌──────────┐              │
                  │   done  │ │ Paused   │─────────────┘
                  │    │    │ └────┬─────┘
                  │    ▼    │      │ resume(Terminate)
                  │ ┌─────┐ │      ▼
                  │ │Compl.│ │ ┌──────────────┐
                  │ │ eted │ │ │ Terminated   │
                  │ └─────┘ │ └──────────────┘
                  ▼ (ch=failed & skip_failed)
            ┌────────────────┐
            │ ch:Skipped     │
            │ (留 Running)   │
            └────────────────┘
                       ▲
                       │
                  on_failure_policy=skip_failed:
                  ch 标 skipped, 继续 dispatch; batch 留 Running
                  （见 §5.6）
```

**补充**：`ch='failed' & terminate` 触发时 batch 直接到 `Terminated`（同 batch 后续章节 cancelled）；路径图省略以保持可读性，语义见 §5.6。

### 5.4 chapter 状态机（变更）

现有 5 态：`Pending / Running / Done / Failed / Cancelled`
**新增 `Skipped`**：on_failure_policy=SkipFailed 时失败章被打上 Skipped（error 字段保留失败原因）。

### 5.5 数据流 · Enqueue（创建批号）

```
UI → create_batch { tnId, label, onFailurePolicy, chapterIds[] }
  ↓
Backend:
  1. tx {
       INSERT batches (status='pending', on_failure_policy=X, created_at=NOW)
       INSERT N × transformation_chapters (status='pending', batch_id=new_id)
     }
  2. scheduler.create_batch():
       对每个 chapter_ids[i]:
         INSERT tc row (新 batch 内) 已完成
         计算 frontier_for(chapter_id_i)   // 见 §5.8
         计算 style_ref_for(chapter_id_i)  // 见 §5.9
         UPDATE batches SET started_at=NOW WHERE id=batch_id  // 首章 dispatch 时
         UPDATE batches SET status='running' WHERE id=batch_id
         JobQueue.enqueue(NewTransformationChapter { ... 已有 9 列, batch_id, style_ref_chapter_id })
```

### 5.6 数据流 · Failure 分流

```
on_chapter_failed(batch, ch, err):
  match batch.on_failure_policy:
    PauseAndReview:
      ch row (status='failed', error=err)
      batch row (status='paused')           // 不 dispatch，等待 resume
    Terminate:
      ch row (status='failed', error=err)
      同 batch 后续 pending chapters → (status='cancelled')
      batch row (status='terminated', ended_at=NOW)
    SkipFailed:
      ch row (status='skipped', error=err)
      取 batch 中下一个 chapter → dispatch 同 on_chapter_done 流程（仍 Running）
```

**§5.6.1 Completed 判据（在 on_chapter_done / on_chapter_failed 末尾检查）**

```
batch → completed 当且仅当:
  - 批次内不存在 status ∈ {pending, running, failed} 的 chapter 行
  - 且至少一行 status = 'done'
batch → terminated 当且仅当:
  - 批次内不存在 status ∈ {pending, running, failed} 的 chapter 行
  - 且全无 status = 'done'（整批全部 cancelled / skipped）
```

即："完成" = 至少 1 章真转换成功；"终止（按 §5.6 Terminate 路径）"已显式设定。其他边界（skip_failed 最后 1 章失败 → 0 done）由 §5.6.1 落到 Terminated；UI 上会有警示"批号未真发生转换"。

### 5.7 数据流 · Resume

```
UI → resume_batch(batchId, action { kind: 'retry'|'skip'|'terminate', chapterId? })
  ↓
scheduler.resume():
  match action:
    Retry(ch_id):
      ch row (status='pending', error=NULL, started_at=NULL, completed_at=NULL)
      batch row (status='running', ended_at=NULL)
      立即 dispatch this chapter（绕过 batch 内部队列头）
    Skip(ch_id):
      ch row (status='skipped')
      batch row (status='running')
      dispatch batch 的下一个章节（同 done 流程）
    Terminate:
      后续 pending chapters → (status='cancelled')
      batch row (status='terminated', ended_at=NOW)
```

### 5.8 Frontier 计算（SQL）

每次 worker 开工前，在事务内执行：

```sql
SELECT tc.id, tc.result_content
  FROM transformation_chapters tc
  JOIN chapters c ON c.id = tc.chapter_id
 WHERE tc.transformation_novel_id = ?
   AND tc.status = 'done'
   AND c.chapter_idx < ?      -- 当前章节 idx
 ORDER BY c.chapter_idx DESC
 LIMIT 1
```

- 跨 batch 取（同 tn 内全局）
- 无结果（首次转换） → prompt 不带 prev 上下文；非错误

### 5.9 Style reference 计算（SQL）

```sql
SELECT result_content FROM transformation_chapters
 WHERE chapter_id = ? AND status = 'done'
 ORDER BY id DESC LIMIT 1
```

- 取该参考章节最近一次成功结果
- 无结果（style_ref 章节从未转过） → prompt 不带 ref；记录 warning（暂不写 chapter row）

### 5.10 与 JobQueue 的耦合点

- JobQueue 的任务完成回调（已有 `notify`）→ `scheduler.on_chapter_done/failed`
- `NewTransformationChapter` 增 `batch_id: Option<i64>` + `style_ref_chapter_id: Option<i64>`
- worker 渲染 prompt 时由 transformer 模块（已存在）拼 frontier + style_ref 段落

---

## 6. IPC 命令（src-tauri/src/commands/batches.rs）

```rust
#[tauri::command]
async fn list_batches(tn_id: i64) -> Result<Vec<Batch>, Error>;

#[tauri::command]
async fn get_batch(batch_id: i64) -> Result<Batch, Error>;

#[tauri::command]
async fn create_batch(payload: CreateBatchPayload) -> Result<Batch, Error>;
// payload: { tn_id, label?, on_failure_policy, chapter_ids }

#[tauri::command]
async fn update_batch(
    batch_id: i64,
    payload: UpdateBatchPayload,
) -> Result<Batch, Error>;
// payload: { label?, on_failure_policy? }（不能在 batch 'running' 时改 policy）

#[tauri::command]
async fn resume_batch(
    batch_id: i64,
    action: ResumeActionPayload,
) -> Result<Batch, Error>;
// action: { kind: 'retry'|'skip'|'terminate', chapter_id?: i64 }

#[tauri::command]
async fn list_batch_chapters(batch_id: i64) -> Result<Vec<TransformationChapterRow>, Error>;

#[tauri::command]
async fn count_batches_by_status(tn_id: i64) -> Result<BatchStatusCount, Error>;
// { pending, running, paused, completed, terminated, cancelled }
```

`lib.rs` 注册到 `invoke_handler!`。`BatchScheduler` 作为 `tauri::State` 提供给命令 closure。

---

## 7. 前端

### 7.1 类型（src/ipc/types.ts）

```typescript
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

export type ResumeAction =
  | { kind: 'retry' | 'skip'; chapter_id: number }
  | { kind: 'terminate' };
```

### 7.2 IPC wrappers（src/ipc/commands.ts）

加 7 个 wrapper，外层 invoke 名是 `list_batches` 等 camelCase 入参（`tnId` / `batchId` / `onFailurePolicy`）。具体命名遵循 `src/ipc/commands.ts` 顶部约定。

### 7.3 Pinia store（src/stores/batches.ts）

```typescript
export const useBatchesStore = defineStore('batches', () => {
  const byTn = ref<Map<number, Batch[]>>(new Map());
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function loadByTn(tnId: number) { /* invoke list_batches */ }
  async function resume(batchId: number, action: ResumeAction) { /* invoke resume_batch */ }
  async function refresh(batchId: number) { /* invoke get_batch */ }

  return { byTn, loading, error, loadByTn, resume, refresh };
});
```

### 7.4 TnDialog 增量（TransformationNovelDialog.vue）

表单顶部（已有 `dataAssetId` + `title` 行）下方增加：

| 字段 | 控件 | 选项 | 校验 |
|------|------|------|------|
| 默认模型 | select | `modelConfigs[]`（来自现有 store） | 必填 |
| 转换类型 | radio | `压缩` / `文风`（值 compress / style） | 必填 |
| 默认 prompt | select | `prompts[]`，按选定 mode 过滤 | 必填 |

提交 payload 加：

```typescript
{
  data_asset_id, title,
  default_model_config_id, default_prompt_id, default_mode,
}
```

### 7.5 路由与入口

- `src/router/index.ts` 加 `{ path: '/library/transformation/:tnId', name: 'transformation-detail', component: () => import('../views/TransformationNovelDetail.vue') }`
- `src/views/Library.vue` 的 `transformations` tab 行加 "详情" 按钮 → `router.push(...)`

### 7.6 TransformationNovelDetail.vue（两 tab 骨架）

```
<PageHeader title=tn.title subtitle=tn.id + 状态 tags>
<Tabs [章节一览 | 工作流]>
  ├ 章节一览 Table:
  │ 列: idx | title | mode | status | prompt_id | batch_id | error | tokens_in/out
  │   ⚠ B-scope 不加 checkbox、不加"新建批量"按钮
  └ 工作流 Table:
      列: label | on_failure_policy | status | created_at | ended_at | progress (N/M)
      行点击 → 侧滑 panel 显示 list_batch_chapters(batchId)
      若 batch.status='paused' → 顶部红条 + Retry/Skip/Terminate 按钮 → resume_batch
```

轮询策略：mount 时 `loadByTn` + `setInterval(5000)` 监听 batch 状态变化；store 暴露 `refresh(batchId)`。

---

## 8. 错误处理

| 场景 | 行为 |
|------|------|
| 章节失败 + PauseAndReview | chapter Failed + batch Paused；UI 不弹错，由用户在 Paused 面板处理 |
| 章节失败 + Terminate | 其他章节 Cancelled + batch Terminated；UI 提示"批号被终止" |
| 章节失败 + SkipFailed | chapter Skipped + 继续；UI 显示 Skipped 行带 error 明细 |
| `create_batch` 参数错（tn_id 无效） | Typed error → `store.error` 显示 |
| Frontier 查询结果空 | prompt 不拼 prev 段落；非错误 |
| Style_ref 章节从未转过 | prompt 不拼 ref 段落；非错误（记 debug 日志） |
| DB 锁冲突 | rusqlite 抛 SQLITE_BUSY → IPC error variant → UI 提示可重试 |
| Worker 重启中途 | batch 由 DB 持久化；重启后 Paused 不自动续跑（手动 resume） |
| `update_batch` 改 policy 但 batch 正在 Running | typed error `BatchNotMutable` |
| `resume_batch` 时 batch 不是 Paused | typed error `BatchNotPaused` |

---

## 9. 测试策略

### 9.1 Rust 新增

- `crates/nsc-core/tests/db_batch.rs` —— CRUD、status 转换、on_failure_policy 列对应、migration 0007 兼容
- `crates/nsc-core/tests/scheduler.rs` —— `BatchScheduler` 单元（fake `JobQueue` + memory DB），覆盖：
  - 首章 dispatch 后 batch 转 Running
  - 多 batch 并行（独立调度）
  - SkipFailed 下失败章→Skipped、下一章继续
  - PauseAndReview 下失败章→Paused、不再 dispatch
  - Terminate 下后续章节 cancelled、batch Terminated
  - resume(Retry) → chapter 重新入队
  - resume(Skip) → chapter Skipped + 下一章继续
  - resume(Terminate) → 后面的 cancelled
- `crates/nsc-core/tests/transformer.rs` 增 frontier / style_ref SQL 测（fixture：1~10 done，转 ch15 拿 ch10；空 frontier 不抛错）

### 9.2 前端新增

- `src/__tests__/batches.spec.ts` —— 7 个 IPC wrapper 的 invoke 形状（精确 camelCase/snake_case 校对）
- `src/__tests__/tn_dialog.spec.ts` —— mode 切换触发 prompt 列表过滤；提交 payload 字段完整

### 9.3 E2E

保持 `test.skip` 占位（沿用现有约定）。

---

## 10. 实现切片（6 片纵向）

按"端到端最小闭环可验证"切。每片独立编译运行、独立可 review。

| Slice | 范围 | 验证手段 |
|-------|------|----------|
| 1. **tn 字段接入** | migration 增列 + Repo upsert/get + IPC `upsert_transformation_novel` 增字段 + TnDialog 3 字段 | cargo test db_transformation + vitest tn_dialog |
| 2. **batches 接入** | migration 0007 batches + `BatchRepo` + 6 个 IPC（除 resume） + useBatchesStore loadByTn + 类型定义 | cargo test db_batch + vitest batches |
| 3. **chapter batch_id 接入** | migration 增 2 列 + `NewTransformationChapter` 增字段 + 现有 enqueue stamp batch_id | cargo test transformer |
| 4. **BatchScheduler 核心** | `batch_scheduler.rs` 模块 + lib.rs 接线 + frontier SQL + style_ref SQL + JobQueue on-finish 回调接通 | cargo test scheduler + transformer |
| 5. **on_failure_policy + resume** | chapter Skipped 状态 + paused 路径 + `resume_batch` IPC + UI 工作流 panel 的 retry/skip/terminate 按钮 | cargo test scheduler + vitest batches |
| 6. **TN 详情页骨架** | 路由 + `TransformationNovelDetail.vue` 两 tab + Library "详情" 入口 + 5s 轮询 + paused 顶部红条 | vitest 组件快照（项目首批 vue-test 模式，DI library 不在此引入） |

---

## 11. 可裁剪点（按代价由小到大）

如果时间不够，按以下顺序裁：

1. **裁 工作流 tab 的 batch 点击侧滑 panel** —— 只显示 batch 表，详情面板留到下 spec
2. **裁 style_ref_chapter_id** —— 列保留但 UI 不暴露选择入口
3. **裁 resume_batch 的 Skip 行为** —— paused 时只允许 Retry 与 Terminate
4. **裁 batches.label** —— 列保留但 UI 不展示
5. **裁 章节一览 table 的完整列** —— 只显示 idx/title/status

**绝不能裁**：

- `batches` 表本身
- `transformation_chapters.batch_id` 列
- `BatchScheduler` 模块（核心调度）
- `on_failure_policy` 三个分支（含暂停/终止/跳过）—— 约定核心
- frontier SQL —— 约定核心
- `default_mode` —— dialog 模式过滤的前提

---

## 12. 已知风险

1. **BatchScheduler 单例 vs 多 Db 连接**：scheduler 持 `db_path: PathBuf`，所有事务都在工厂内 `Db::open` 后短生命周期持有（沿用项目不变量）
2. **batch.status 并发写**：running 状态由 scheduler 写入；用户在 batch Running 时改 on_failure_policy 的入口要禁止（见 §8 `BatchNotMutable`）
3. **Paused 行刷新频率**：detail page 5s 轮询；若高频被证实必要，再考虑 tauri event 推送
4. **章节一览 table 在大数据集下性能**：典型 200~500 章，先按 tn + status 索引；后续若卡可加 LIMIT + 虚拟滚动
5. **TnDialog mode 过滤 prompt 的 loading 时机**：dialog 打开时先全量 load prompts，再按 mode 过滤；prompts 表小（典型 < 20 条）成本可忽略
6. **存量散点 transformation_chapters.batch_id = NULL**：UI 兜底"未指定批号 / 历史散点"显示；不影响新流程

---

## 13. 完成定义（DoD）

- migration 0007 可重放（IF NOT EXISTS）
- cargo test 全部通过（含新增 3 个测试文件）
- vitest 全部通过（含新增 2 个测试文件）
- 用户可在 Library 创建带 mode + prompt + model 的 tn
- 用户可在 TN 详情页看到两 tab 的内容
- 用户可经 IPC 触发 create_batch（命令面板或测试脚本）→ 章节按 frontier 串行完成
- 模拟 chapter 失败时，batch 按 policy 拍板为 Paused / Terminated / 继续（Skipped）
- 用户调 resume_batch → batch 状态正确恢复 / 终止
- 所有不变量经 audit（CLAUDE.md §"Critical invariants"）未破坏
