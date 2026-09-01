# Stopped Batch 追加章节 + 继续执行 设计

**状态**: 待用户审批
**日期**: 2026-08-26

---

## 1. 背景

### 1.1 业务痛点

用户在「转换工程」详情页打开某个工作流详情时,如果该工作流已停止(`stopped`),目前**没有任何入口**可以「补充后续章节并继续跑」。

唯一近似的能力是「重试」(retry),但 retry 仅作用于 batch 已有的 failed/skipped 章节,**不能从 source data_asset 拉新章节到 batch**。

实际场景:长篇小说转换到 50% 用户主动停;后面想接着转换剩余 50%,只能新建一条 workflow,从 UX 视角看「这是同一个工作的后半段」,但 DB 视角被强行拆成两条 workflow、`total_count` 拆开、跨章节上下文割裂。

### 1.2 业务目标

**让 stopped batch 可以从 source data_asset 拉新章节追加,然后 batch 自动从 stopped 转 running 继续执行**。

- 用户视角:单一动作「补充章节」= 同时表达「加章节」+「继续跑」
- 状态机视角:`stopped → running` 单一转移,不引入半成品中间态
- 数据视角:复用 batch 已有的 prompt/model/ctx 配置(用户原话:严格复用),不暴露编辑
- 跨章节上下文:后半章的 prev_original/prev_transformed 自动从 batch 已转换章节读取(已有机制,JobSpec 自带 ctx_*)

### 1.3 不做什么

- 不让 `completed` / `terminated` / `cancelled` append。这些是真正终态,append 不改语义 —— 用户走「新建续工作流」路径。
- 不让 `running` / `paused` append。worker race 太大,另开 spec。
- append 时不让用户改 prompt/model。复用 batch 现成配置,UI 不暴露编辑。
- 不做「append 时跨 batch 复制 prompt 配置」抽象。

---

## 2. 核心模型

### 2.1 单一意图 → 单一状态转移

「stopped batch 追加章节并继续」= 一个 IPC 命令 + batch 状态 `stopped → running` 单一转移。

```
append_chapters_to_batch(batch_id, chapter_ids)
  ↓ 后端
  1. 校验 batch.status == Stopped(其他状态全部 Err)
  2. 校验 chapter_ids 全部属于 batch.tn.data_asset
  3. 去重:剔除已在 batch 中(batch.tc.chapter_id) 的章节
  4. 事务内:
     a. INSERT transformation_chapters 行 (status='pending')
     b. INSERT OR IGNORE workflow_result_chapters 空槽
     c. UPDATE batches SET status='running', started_at=COALESCE(started_at, now), ended_at=NULL
  5. 入队全部新章节到 JobQueue(用 batch 现成 prompt/model/ctx)
  6. 返回 { batch_id, added_chapter_ids, status: Running }
```

**为什么 batch 状态必须从 stopped 转 running**:

- 「append 后继续跑」是单一动作 → 状态机也必须单一转移
- 停在 `stopped` 但有未跑槽(中间态) = 制造新「半成品」状态,Bug 滋生地
- `running` 自动被现有轮询/UI 监测,用户立刻看到 batch 又活了

### 2.2 batch 同质配置前提

现有 batch 表 schema 不存 prompt/model/ctx/mode(这些字段只存在 `transformation_chapters` 行上)。append 时需要从 batch 拿配置,有两条路:

- **(a) 从 batch 任意一个现有 tc 行反查** —— 缝补,违背 CLAUDE.md「Surgical Changes」原则
- **(b) 给 batches 表加同质配置列** —— 治本,业务上 batch 本来就该同质(同一次 enqueue 用同一套配置)

采用 **(b)**:schema 补齐,append 路径读 batch 字段即可,无反查、无 race。

---

## 3. 后端改动

### 3.1 schema migration

`migrations/0029_batch_homogeneous_config.sql`(新建):

```sql
-- batches 补「同质配置」字段:batch 创建时统一采用同一套 prompt/model/ctx/mode。
-- stopped batch append 章节时,从 batch 字段直接读,无需反查 tc 行。
ALTER TABLE batches ADD COLUMN prompt_id INTEGER;
ALTER TABLE batches ADD COLUMN model_config_id INTEGER;
ALTER TABLE batches ADD COLUMN mode TEXT;
ALTER TABLE batches ADD COLUMN ctx_prev_original INTEGER;
ALTER TABLE batches ADD COLUMN ctx_prev_transformed INTEGER;
ALTER TABLE batches ADD COLUMN ctx_next_original INTEGER;
ALTER TABLE batches ADD COLUMN ctx_next_transformed INTEGER;

-- 旧数据 backfill:从该 batch 下任意一个 tc 行取(业务上同质)。
-- 不存在 tc 行的 batch 留 NULL(理论上不该有;防御性)。
UPDATE batches SET
  prompt_id = (SELECT prompt_id FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  model_config_id = (SELECT model_config_id FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  mode = (SELECT mode FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  ctx_prev_original = (SELECT ctx_prev_original FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  ctx_prev_transformed = (SELECT ctx_prev_transformed FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  ctx_next_original = (SELECT ctx_next_original FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  ctx_next_transformed = (SELECT ctx_next_transformed FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1);
```

`crates/nsc-core/src/db/migrate.rs` 注册新 schema。

### 3.2 models

`crates/nsc-core/src/models/batch.rs`:

```rust
pub struct NewBatch {
    pub transformation_novel_id: i64,
    pub label: Option<String>,
    pub on_failure_policy: OnFailurePolicy,
    // 新增:
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub mode: PromptKind,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
    pub ctx_next_transformed: i32,
}
```

`Batch` model 同步加 7 个字段(读 batch 行时一并 SELECT)。

### 3.3 repo

`crates/nsc-core/src/db/repo/batch.rs`:

- `insert` SQL 加 7 列
- `batch_from_row` 读第 8-14 列(prompt_id ... ctx_next_transformed)
- `list_*` SELECT 列表同步加 7 列
- `set_status` 不变(本任务未触)
- 新增 `BatchRepo::get(id)` if 缺失(append_chapters 需要读 batch 校验状态)

### 3.4 `append_chapters_to_batch` IPC 命令

`src-tauri/src/commands/transformations.rs`(新增):

```rust
#[derive(Debug, Serialize)]
pub struct AppendChaptersResult {
    pub batch_id: i64,
    pub added_chapter_ids: Vec<i64>,
    pub status: BatchStatus,
}

#[tauri::command]
pub fn append_chapters_to_batch(
    db: State<'_, Arc<Db>>,
    job_queue: State<'_, Arc<JobQueue>>,
    batch_id: i64,
    chapter_ids: Vec<i64>,
) -> Result<AppendChaptersResult, String> {
    // 1. 进 IMMEDIATE 事务,锁住 batch 行(防 stop/append 并发)
    let tx = db.lock().unchecked_transaction().map_err(|e| e.to_string())?;

    // 2. 读 batch + 校验 status
    let batch = /* SELECT FROM batches WHERE id = ?1 */
        .ok_or_else(|| format!("batch {batch_id} 不存在"))?;
    if batch.status != BatchStatus::Stopped {
        return Err(format!("仅 stopped 工作流可追加章节(当前 {:?})", batch.status));
    }

    // 3. 读 tn + 校验 chapter_ids 都属于该 da
    let tn = /* SELECT FROM transformation_novels WHERE id = batch.tn_id */
        .ok_or_else(|| format!("tn {} 不存在", batch.transformation_novel_id))?;
    let da_chapter_set: HashSet<i64> = db.chapters().list_by_data_asset(tn.data_asset_id)
        .map_err(|e| e.to_string())?
        .iter().map(|c| c.id).collect();
    for &cid in &chapter_ids {
        if !da_chapter_set.contains(&cid) {
            return Err(format!("chapter {cid} 不属于本 tn 的 data_asset {}", tn.data_asset_id));
        }
    }

    // 4. 去重:剔除已在 batch 中的章节
    let existing: HashSet<i64> = db.transformation_chapters().list_by_batch(batch_id)
        .map_err(|e| e.to_string())?
        .iter().map(|tc| tc.chapter_id).collect();
    let to_add: Vec<i64> = chapter_ids.iter().copied().filter(|c| !existing.contains(c)).collect();
    if to_add.is_empty() {
        return Err("所选章节全部已在工作流中".to_string());
    }

    // 5. 校验 batch 的 prompt/model 字段存在(backfill 后必填,但 tc 表 fallback 不存在时仍是 NULL)
    let prompt = db.prompts().get(batch.prompt_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("prompt {} 不存在", batch.prompt_id))?;
    let model = db.model_configs().get(batch.model_config_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("model_config {} 不存在", batch.model_config_id))?;

    // 6. 插 transformation_chapters + workflow_result_chapters 空槽
    let now = chrono::Utc::now().to_rfc3339();
    let mut tc_ids: Vec<i64> = Vec::with_capacity(to_add.len());
    for &cid in &to_add {
        tx.execute(
            "INSERT INTO transformation_chapters \
             (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
              ctx_prev_original, ctx_prev_transformed, ctx_next_original, ctx_next_transformed, \
              batch_id, status, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending', ?11)",
            params![batch.transformation_novel_id, cid, /* mode */,
                    batch.prompt_id, batch.model_config_id,
                    batch.ctx_prev_original, batch.ctx_prev_transformed,
                    batch.ctx_next_original, batch.ctx_next_transformed,
                    batch_id, now],
        ).map_err(|e| e.to_string())?;
        tc_ids.push(tx.last_insert_rowid());
    }
    db.workflow_results().create_for_batch_with_slots(batch_id, &to_add)
        .map_err(|e| e.to_string())?;

    // 7. batch 状态迁移 stopped → running
    /* UPDATE batches SET status='running', started_at=COALESCE(started_at, now), ended_at=NULL WHERE id=?1 */

    tx.commit().map_err(|e| e.to_string())?;

    // 8. 入队(事务外;JobQueue 独立通道)
    for (tc_id, chapter_id) in tc_ids.iter().zip(to_add.iter()) {
        job_queue.enqueue(/* JobSpec { tc_id, chapter_id, prompt, model, ctx_*, ... } */);
    }

    Ok(AppendChaptersResult { batch_id, added_chapter_ids: to_add, status: BatchStatus::Running })
}
```

### 3.5 `lib.rs` 注册

`src-tauri/src/lib.rs` `invoke_handler!` 注册 `append_chapters_to_batch`。

---

## 4. 前端改动

### 4.1 IPC 类型 + wrapper

`src/ipc/types.ts`:

```ts
export type AppendChaptersToBatchPayload = {
  batchId: number;
  chapterIds: number[];
};

export interface AppendChaptersResult {
  batch_id: number;
  added_chapter_ids: number[];
  status: WorkflowStatus;
}
```

`src/ipc/commands.ts`:

```ts
export function appendChaptersToBatch(payload: AppendChaptersToBatchPayload): Promise<AppendChaptersResult> {
  return invoke<AppendChaptersResult>('append_chapters_to_batch', {
    batchId: payload.batchId,
    chapterIds: payload.chapterIds,
  });
}
```

### 4.2 store action

`src/stores/workflows.ts` 新增:

```ts
async function appendChapters(batchId: number, chapterIds: number[]): Promise<AppendChaptersResult> {
  const res = await ipcAppendChaptersToBatch({ batchId, chapterIds });
  queryClient.invalidateQueries({ queryKey: ['workflowChapters', res.batch_id] });
  queryClient.invalidateQueries({ queryKey: ['workflows', tnId] });
  return res;
}
```

### 4.3 `AppendChaptersDialog.vue`(新组件)

复用 `TransformationNovelDetail.vue` 的章节选择机制(selectedChapterIds + rangeFrom/To + replace/toggle),但**对话框独立**。

UI 草图:

```
┌──────────────────────────────────────────────┐
│  补充章节 — {workflow.label}                 │
├──────────────────────────────────────────────┤
│  配置(来自本工作流,不可改):                  │
│    • Prompt: {prompt.name}                    │
│    • Model:  {model.display_name}             │
│    • Mode:   {mode}                            │
│    • 上下文: 上{prev_o}/上{prev_t}/下{next_o}  │
├──────────────────────────────────────────────┤
│  选章节(从 {N} 章源里,排除已在工作流的):     │
│  ┌──────────────────────────────────────┐    │
│  │ #1 □ 第一章:开篇(已在工作流, 不可选)│    │
│  │ #2 □ 第二章今世只想生孩子             │    │
│  │ #3 □ 第三章:误会                     │    │
│  └──────────────────────────────────────┘    │
│                                              │
│  范围: [_] → [_] (replace/toggle)            │
│  [全选] [清空]                                │
│                                              │
│  已选 N 章                                    │
│                                              │
│            [取消]    [确认补充并执行 (N 章)] │
└──────────────────────────────────────────────┘
```

**关键行为**:

1. **配置只读**(严格复用 batch 配置)
2. **已在工作流的章节 disabled**(checkbox 不可勾)+ tooltip「已在工作流中」
3. **范围选择 / 全选 / 清空**:复用 `TransformationNovelDetail.vue` 的 `applyRange` / `selectAll` / `selectNone` 逻辑(抽出来或 dialog 内 copy)
4. **确认按钮 disabled 当 selectedChapterIds.size === 0**
5. **后端报错 → 弹 alert dialog**(复用父组件 `showAlert` 或 emit)
6. **loading 态**:`onConfirm` 期间按钮 loading + disable
7. **打开时 sources 拉取**:`listTransformationSourceChapters(tnId)`(已存在 IPC)
8. **打开时 batch 现章节拉取**:`listWorkflowChapters(batchId)`(已存在 IPC)→ 拿 current chapter_ids 集合

### 4.4 `TransformationNovelDetail.vue` 改动

- **workflow 详情 actions 列**(workflowChapterColumns.actions):新增「补充章节」按钮,按 batch.status 分支:
  - `stopped`:显示,主操作
  - 其他:不显示
- `openAppendChaptersDialog(batch)` 函数
- `appendOpen` ref + `appendChaptersDialog` 组件挂载
- 后端报错 → `showAlert('补充失败', msg)`

---

## 5. Fail-fast 契约

| 场景 | 行为 | 谁负责 |
|---|---|---|
| batch 不存在 | `Err("batch {id} 不存在")` | 后端 |
| batch.status != Stopped | `Err("仅 stopped 工作流可追加章节(当前 {status})")` | 后端 |
| chapter_id 不属于 batch.tn.data_asset | `Err("chapter {cid} 不属于本 tn 的 data_asset {da_id}")` | 后端 |
| chapter_ids 全部已在 batch 中 | `Err("所选章节全部已在工作流中")` | 后端 |
| batch.prompt_id / model_config_id 缺失 | `Err("prompt {id} 不存在")` / `Err("model_config {id} 不存在")` | 后端 |
| prompt 已被归档 | 不阻止 append;worker 跑时 fail-fast(已有机制) | worker |
| fronted selectedChapterIds 含已在 batch 的章节 | 后端去重后只入队「真正新加的」;前端 UI disabled 防触发 | 后端兜底 |
| 并发:append 与 stop/set_status 同时 | IMMEDIATE 事务 + 行锁,后到者 Err "batch 状态已变" | 后端 |
| `chapter_ids` 为空 | `Err("至少选 1 章")` | 后端 |

**不静默,不 fallback**:失败点一律抛错,带诊断信息(id / 当前 status / 来源链)。

---

## 6. 状态机迁移图

```
                     ┌──────────────────────────┐
                     │ enqueue_transformation_chapters │
                     │ (running+0 in-flight / paused+0 in-flight)
                     │          ↓                  │
                     │     running(新 tc 槽已建)  │
                     └─────────┬─────────────────┘
                               │
              ┌────────────────┼─────────────────┐
              ↓                ↓                 ↓
          stopped         completed       terminated/cancelled
       (user stop 或                                │
        no in-flight                              │
        pause 后的最终态)                            │
              ↑                                      │
              │                                      │
   append_chapters_to_batch                        │
   (status → running)                               │
                                                    │
              ┌──────────────────────────────────────┘
              │  user 主动新建 workflow(UI fallback)
              │  「工作流已 completed,无法追加章节,[新建续工作流]」→CreateBatchDialog
              └──────────────────────────────────────
```

`stopped` 是**唯一**可 append 的状态。

---

## 7. 测试

### 7.1 Rust(`crates/nsc-core/tests/append_chapters.rs` 新建)

```rust
#[test]
fn append_to_stopped_batch_succeeds() { /* 准备 stopped batch,append 2 章,验证 status=running + tc +2 + wr_chapters +2 */ }

#[test]
fn append_to_running_batch_rejected() { /* running batch append → Err "仅 stopped" */ }

#[test]
fn append_to_completed_batch_rejected() { /* completed batch append → Err */ }

#[test]
fn append_to_pending_batch_rejected() { /* pending batch append → Err */ }

#[test]
fn append_with_wrong_data_asset_chapter_rejected() {
    /* 准备 stopped batch,append 别的 tn 的 chapter → Err "不属于本 tn" */
}

#[test]
fn append_all_duplicates_rejected() {
    /* stopped batch 含 [#1],append [#1] → Err "全部已在工作流" */
}

#[test]
fn append_partial_duplicates_succeeds_with_subset() {
    /* stopped batch 含 [#1],append [#1, #2] → added=[#2] */
}

#[test]
fn append_concurrent_with_status_change() {
    /* 模拟:append 期间 set_status(stopped→running) 抢先;
       期望后到者 Err */
}

#[test]
fn append_uses_batch_homogeneous_config() {
    /* stopped batch 配置 A,append 时 tc 行的 prompt/model/ctx 全部 = A */
}

#[test]
fn batch_insert_round_trips_with_homogeneous_config() {
    /* 准备 stop 后回看 batch 行:7 个新字段都正确 */
}
```

### 7.2 前端 vitest

`src/__tests__/workflows.spec.ts`(扩或新建):

```ts
describe('workflows store: appendChapters', () => {
  it('appendChapters 调用正确 IPC', async () => {
    await store.appendChapters(1, [10, 11]);
    expect(ipcAppendChaptersToBatch).toHaveBeenCalledWith({ batchId: 1, chapterIds: [10, 11] });
  });

  it('appendChapters 后 invalidate 相关 query', async () => {
    await store.appendChapters(1, [10]);
    expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['workflowChapters', 1] });
    expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['workflows', /* tnId */] });
  });

  it('appendChapters 失败错误冒泡', async () => {
    (ipcAppendChaptersToBatch as Mock).mockRejectedValueOnce(new Error('仅 stopped 工作流'));
    await expect(store.appendChapters(1, [10])).rejects.toThrow('仅 stopped');
  });
});

describe('AppendChaptersDialog', () => {
  it('打开时 sources 拉取 + 过滤已在 batch 的章节', async () => {
    // mock listTransformationSourceChapters → 3 章
    // mock listWorkflowChapters → batch 含 #1
    // 断言:dialog 显示 3 章,#1 disabled,其他可选
  });

  it('已选 0 章时确认按钮 disabled', async () => {
    // selectedChapterIds.size === 0 → 按钮 disabled
  });

  it('点击确认调用 store.appendChapters', async () => {
    // 选 #2,#3 → 调 store.appendChapters(batchId, [2, 3])
  });

  it('后端报错时弹 alert', async () => {
    // mock appendChaptersToBatch 抛 '仅 stopped' → alert 触发
  });
});
```

### 7.3 E2E(placeholder)

`tests-e2e/append-chapters.spec.ts`(新建):

```ts
test.skip('stopped batch append chapters triggers running transition', async () => { ... });
test.skip('non-stopped batches hide append button', async () => { ... });
```

本地 `test.skip`(无 Tauri runtime)。

---

## 8. 自审清单

- [x] 无 TBD / TODO
- [x] 核心模型单一意图映射到单一状态转移
- [x] schema 补齐(b1 治本路径),非反查缝补
- [x] fail-fast 表格覆盖所有失败路径
- [x] 状态机迁移图明确不变量
- [x] 测试覆盖:Rust 7+ 用例、vitest 5+ 用例、E2E placeholder
- [x] 「不做什么」明确范围:completed/terminated/cancelled 不支持;running/paused 不支持;配置只读
- [x] 与现有 batch 状态机协调:`running+0 in-flight` / `paused+0 in-flight` 仍是合法 enqueue 集(不动);新增 `stopped` 进入 enqueue 集(只在 append 路径,且走完整状态转移)

---

## 9. 后续(独立 spec,不属本次)

- 「append 时改 prompt/model」(用户主动切换配置)
- 「completed 也能 append」(状态机扩 completed → running 转移)
- 「paused / running 也能 append」(worker race 重新设计)
- 「append 后入队时 worker pool 容量调整」
