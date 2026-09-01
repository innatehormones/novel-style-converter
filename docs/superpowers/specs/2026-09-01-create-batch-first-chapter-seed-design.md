# 「新建工作流」试运行区 · 首章种子可选化 — 设计

**日期**：2026-09-01
**状态**：brainstorming 已收敛，待用户阅读 spec
**前置**：
- `docs/superpowers/specs/2026-08-20-create-workflow-preview-design.md`（试运行区原始设计）
- `docs/superpowers/specs/2026-08-14-regenerate-preview-design.md`（单章节重新生成，ai_call_logs 业务类型）
- `crates/nsc-core/src/transformer/batch_scheduler.rs` 的 `create_workflow` 现有路径

**关系**：
- **不替代** `2026-08-20` 原始 spec 的 `previewFirstChapter` IPC；保留
- **不替代** `RegeneratePreviewDialog`（那是工作流跑完后调整单章）
- **本次目标**：把"必须先生成预览才能创建工作流"改成"种子可选"

---

## 1. 目标与动机

### 1.1 当前痛点

`CreateBatchDialog.vue` 的试运行区有一个隐性约束：`previewAccepted.value || !props.previewChapterId`。意思是**只要父组件传了预览章节 id（一定会传），用户就必须走"生成预览 → 点满意"的硬路径才能创建工作流**。

来源是 `2026-08-20` spec §6.3 的设计选择：当时把 previewFirstChapter 视为"用户必须看到 AI 出什么再决定"。实际使用中，用户经常想：

1. **手写派**：AI 跑得不准，自己改得更快 — 不该被迫等一次 LLM 调用
2. **历史派**：用户之前跑过同 prompt/model，已经知道 AI 大概会出什么，但懒得再调一次 — 不该重复花费 token
3. **简路径**：选了章节、prompt、model 之后想直接开跑，看真实运行结果再决定 — 不该被 dialog 拦截

### 1.2 本次范围

**包含**：
- 试运行区 3 区 UI：原文 / 预览 / 转换结果
- "转换结果"区可手写、可从预览复制、可保持空
- 移除"采用/不满意/重新选"按钮对
- 后端 `FirstChapterSeed`（原 `PreviewFirstChapter`）改可空 + 加来源枚举
- `batch_scheduler::create_workflow` 支持 seed=None（首章作为普通 job 入队）

**不包含**：
- 预览章节的切换（仍固定 idx 最小）
- 预览结果的多 tab 历史
- prompt 编辑器内预览（按原始 spec TODO 保留）
- ctx toggle 升级高级模式

---

## 2. 核心业务约束

### 2.1 首章种子的三种来源

| 来源 | 何时产生 | content 来源 | tokens_in/out |
|---|---|---|---|
| `Llm` | 用户点"生成预览" + 点"↑ 从预览复制" | LLM 输出 | LLM 实算 |
| `Manual` | 用户在"转换结果"区手写 | 用户输入 | 0 |
| `None` | 用户没填"转换结果"区 | —（不入库） | — |

**`None` 路径语义**：seed=None 时首章作为普通 TC 行（status='pending'）由 JobQueue 调度，跟其他章节一样等 LLM 处理。这恢复了 `2026-08-20` spec §5.2 "None → 行为不变" 的能力 — 当前实现里被 `previewAccepted` 检查覆盖了。

### 2.2 不卡用户原则

`canSubmit` 不再检查 seed 是否存在。按钮在 label/prompt/model/章节数齐的条件下永远可点。**这是用户在 brainstorming 中明确要求的不变量**：

> "哪怕用户勾选了上、下章，哪怕内容没有，那也是用户自己负责的，他可以直接创建，如内容没有时，我们就走首章创建 LLM 的逻辑"

### 2.3 预览结果不落库

沿用 `2026-08-20` §3.3 / `2026-08-14` §3.2：dialog 内的预览生成是纯前端状态，关闭 dialog 即丢。仅 `previewFirstChapter` IPC 每次调用写一条 `ai_call_logs`（business=RegeneratePreview）。

---

## 3. 数据模型

### 3.1 不新增表 / 不写 schema migration

`transformation_chapters` 表已有 `tokens_in/out` 列，0 是合法值。

### 3.2 后端类型（`crates/nsc-core/src/models/transformation.rs`）

**改名 + 加枚举**：

```rust
/// 「新建工作流」时，用户可选择为首章预置的内容（"种子"）。
/// 可不传（None），此时首章由 LLM 在 batch 内正常处理。
pub struct FirstChapterSeed {
    pub content: String,
    pub source: SeedSource,
}

pub enum SeedSource {
    /// 用户调 previewFirstChapter + 从预览复制 → seed 来自 LLM。
    Llm { tokens_in: i32, tokens_out: i32 },
    /// 用户在 dialog 内手写 → 没有 LLM 调用，tokens 为 0。
    Manual,
}
```

**字段名 `preview_first_chapter` 在 IPC 边界保留**（Tauri 命令 DTO 的字段名跟代码历史命名挂钩，改名波及面过大）。类型从 `PreviewFirstChapter` 改 `FirstChapterSeed`，从必填改 `Option<FirstChapterSeed>`。

### 3.3 前端类型（`src/ipc/types.ts`）

```typescript
export interface FirstChapterSeed {
  content: string;
  source:
    | { kind: 'llm'; tokens_in: number; tokens_out: number }
    | { kind: 'manual' };
}

/// CreateWorkflowInput.preview_first_chapter 类型改为：
preview_first_chapter: FirstChapterSeed | null;
```

字段名 `preview_first_chapter` 保留（同上）。

---

## 4. UI 设计

### 4.1 试运行区三区布局

```
┌─────────────────────────────────────┐
│  原文（第 1 章，只读）               │  ← getChapter
├─────────────────────────────────────┤
│  预览                       [+ 生成预览] │
│  [LLM 输出展示，单次覆盖]            │  ← previewFirstChapter
├─────────────────────────────────────┤
│  转换结果          [↑ 从预览复制][清空] │
│  [可手写 / 可复制 / 可保持空]         │
└─────────────────────────────────────┘
```

**位置不变**：三区从上到下；视觉关系和现状一致（左右两栏，右侧下半区是试运行区）。

### 4.2 状态机

| 触发 | 结果 |
|---|---|
| 打开 dialog | 原文加载；预览空；转换结果空 |
| 手写转换结果区 | `seedContent = 输入`，`seedSource = Manual` |
| 点"生成预览" | 调 `previewFirstChapter` IPC；成功后 `previewOutput = 返回内容`，**不动** seedContent |
| 点"↑ 从预览复制"（previewOutput 空时禁用） | `seedContent = previewOutput`，`seedSource = Llm { tokens_in, tokens_out }` |
| 再次点"↑ 从预览复制"（seedContent 已非空） | confirm 对话框：确定=追加到末尾；取消=替换 |
| 点"清空" | `seedContent = ''`，`seedSource = null` |
| 切换 `previewChapterId` | 清掉 `previewOutput / seedContent / seedSource`；重新拉原文 |
| 重选 prompt/model | **不动** seedContent / previewOutput |
| 点"创建工作流" | `canSubmit` 仅检查 label/prompt/model/章节数；payload.preview_first_chapter 跟随 seedSource |

### 4.3 canSubmit 终态

```typescript
const canSubmit = computed(() =>
  promptId.value !== 0
    && modelConfigId.value !== 0
    && label.value.trim() !== ''
    && props.selectedChapterIds.length > 0
    && !submitting.value,
);
```

移除原 `previewAccepted.value || !props.previewChapterId` 检查。

### 4.4 移除项

- "满意，使用此结果"按钮 → 不再存在
- "已选 ✓ 重新选"按钮对 → 不再存在
- `previewAccepted` / `previewFirstChapterRef` ref → 删除
- `onAcceptPreview` / `onReselectPreview` 函数 → 删除

### 4.5 新增项

- "↑ 从预览复制"按钮（previewOutput 空时禁用）
- "清空"按钮
- "种子来源提示"小字（seedSource=Llm 时显示 `（来自 LLM，消耗 tokens_in/out）`，seedSource=Manual 时显示 `（手写，不消耗 tokens）`）

---

## 5. 后端逻辑

### 5.1 `batch_scheduler::create_workflow` 行为分支

```
match seed {
  None =>
    首章 tc 行与其他章节一样 INSERT(status='pending')
    // JobQueue 会按 frontier 调度，调 LLM 生成
  Some(FirstChapterSeed { content, source: Llm { tokens_in, tokens_out } }) =>
    事务内额外 UPDATE 首章 tc:
      status = 'done'
      result_content = content
      tokens_in = tokens_in
      tokens_out = tokens_out
      started_at = Utc::now()  // 记录手动种子时间
      completed_at = Utc::now()
      error = NULL
    // 其他章节照常 INSERT pending
  Some(FirstChapterSeed { content, source: Manual }) =>
    事务内额外 UPDATE 首章 tc:
      status = 'done'
      result_content = content
      tokens_in = 0
      tokens_out = 0
      started_at = Utc::now()
      completed_at = Utc::now()
      error = NULL
    // 其他章节照常 INSERT pending
}
```

### 5.2 IPC 命令 `preview_first_chapter` 不变

`crates/nsc-core/src/transformer/batch_scheduler.rs::preview_first_chapter` 命令行为不变，仅供前端 dialog 内"生成预览"按钮调用。其返回 `{ content, tokens_in, tokens_out }` 仅作为"转换结果"区复制的来源，**不再**透传到 `create_workflow`（透传的是用户最终的 seedContent + 推断的 source）。

### 5.3 IPC 命令 `create_workflow` 扩展

`src-tauri/src/commands/workflows.rs` 的 DTO `CreateWorkflowInput.preview_first_chapter`：
- 类型 `Option<FirstChapterSeed>`（原 `Option<PreviewFirstChapter>`）
- 字段名保留
- 序列化遵循 snake_case（现有 `#[serde(rename_all = "snake_case")]` 适配）

---

## 6. 文件改动清单

### 6.1 后端

| 文件 | 改动 |
|---|---|
| `crates/nsc-core/src/models/transformation.rs` | `PreviewFirstChapter` → `FirstChapterSeed`；新增 `SeedSource` 枚举 |
| `crates/nsc-core/src/transformer/batch_scheduler.rs` | `create_workflow` 入参 `first_chapter_seed: Option<FirstChapterSeed>`；事务内分支；None 路径恢复"所有 tc pending" |
| `src-tauri/src/commands/workflows.rs` | DTO `preview_first_chapter: Option<FirstChapterSeed>`；serde 标签兼容 |
| `src-tauri/src/commands/transformations.rs` | 如有 preview_first_chapter 内部转换，相应更新类型 |

### 6.2 前端

| 文件 | 改动 |
|---|---|
| `src/ipc/types.ts` | `PreviewFirstChapter` 类型 → `FirstChapterSeed`；加 `SeedSource` 联合类型；`CreateWorkflowInput.preview_first_chapter` 改 nullable |
| `src/components/CreateBatchDialog.vue` | 三区 UI；新增"↑ 从预览复制"+"清空"按钮；删除"采用/重新选"；canSubmit 简化；emit payload 跟随新类型 |

### 6.3 测试

| 文件 | 改动 |
|---|---|
| `crates/nsc-core/tests/transformer_ctx.rs` | 加 `create_workflow_with_llm_seed` / `create_workflow_with_manual_seed` / `create_workflow_with_null_seed` 三个测试；现有 LLM seed 测试改为使用新类型 |
| `src/components/__tests__/CreateBatchDialog.spec.ts`（如不存在则新增） | 9 个 vitest 场景（见 §7） |

### 6.4 不改

- `src-tauri/src/lib.rs` 命令注册
- `src-tauri/tauri.conf.json`
- schema migration（无新表 / 改列）
- `RegeneratePreviewDialog.vue`
- `chapter_previews` 表

---

## 7. 测试策略

### 7.1 后端（`crates/nsc-core/tests/transformer_ctx.rs`）

| 测试名 | 验证点 |
|---|---|
| `create_workflow_with_llm_seed` | seed.source=Llm → 首章 tc `status='done'`、`result_content` 正确、`tokens_in/out` 与 seed 一致；其他章节 tc `pending` |
| `create_workflow_with_manual_seed` | seed.source=Manual → 首章 tc `status='done'`、`result_content = content`、`tokens_in = 0`、`tokens_out = 0` |
| `create_workflow_with_null_seed` | seed=None → 首章 tc `status='pending'`（无 result_content、无 completed_at）；与其他章节一致 |
| `first_chapter_seed_does_not_overwrite_old_done_tc` | 同 tn 已有同名 idx 的旧 done tc 行时，seed=None 不应被新 batch 覆盖；新 batch 插入新 tc 行 |

测试基础设施：`Db::open_in_memory()` + wiremock（模拟 LLM）。

### 7.2 前端（vitest）

`src/components/__tests__/CreateBatchDialog.spec.ts`：

| 场景 | 断言 |
|---|---|
| 默认状态 | 打开时 `seedContent=''`、`previewOutput=''`、`seedSource=null` |
| 提交且 seedContent 为空 | emit payload `preview_first_chapter = null` |
| 手写后提交 | emit payload `preview_first_chapter = { content, source: { kind: 'manual' } }` |
| 生成预览 + 复制后提交 | emit payload `preview_first_chapter.source = { kind: 'llm', tokens_in, tokens_out }` |
| 切换 `previewChapterId` | `seedContent` / `previewOutput` 被清空 |
| 重选 prompt/model | `seedContent` **不被清** |
| "↑ 复制"按钮禁用 | `previewOutput.trim() === ''` 时 `disabled` |
| "清空"按钮 | `seedContent = ''`、`seedSource = null` |
| canSubmit 永真（除基础必填外） | 不再检查 `previewAccepted` 或 `seedContent` 非空 |

mock `previewFirstChapter` IPC 返回固定 fixture。

### 7.3 回归保证

- **现有 LLM seed 测试路径**继续 work（字段名 `preview_first_chapter` 保留，类型变 `FirstChapterSeed | null`，形状兼容）
- **`RegeneratePreviewDialog` 不受影响**——它走 `chapter_previews` 表，与 `first_chapter_seed` 无关
- **`2026-08-20` spec §5.2 "None → 行为不变"**本次被显式实现；原 spec 该路径被 `previewAccepted` 隐式屏蔽，本次解除屏蔽

### 7.4 不测

- E2E（`tests-e2e/` 是 placeholder）
- 性能 / 负载

---

## 8. 实施步骤（按可独立交付拆分）

| 步骤 | 内容 | 依赖 | 验证 |
|---|---|---|---|
| 1 | 后端 models：改名 + 加枚举 | — | `cargo build -p nsc-core` 通过 |
| 2 | 后端 `batch_scheduler::create_workflow`：支持 seed=None | 1 | 现有 LLM seed 测试继续绿 |
| 3 | 后端 commands/workflows：DTO nullable | 1, 2 | `cargo build -p nsc-core` 通过 |
| 4 | 前端 `ipc/types.ts`：类型改 | 1 | `pnpm test` 不报错 |
| 5 | 前端 `CreateBatchDialog.vue`：三区 UI + 按钮改 | 4 | `pnpm test` 通过；手动 dialog 打开正常 |
| 6 | 后端测试：4 个测试 | 3 | `cargo test -p nsc-core` 全绿 |
| 7 | 前端 vitest：9 个场景 | 5 | `pnpm test` 全绿 |
| 8 | 全套验证 | 6, 7 | `pnpm test` + `cargo test -p nsc-core` + `pnpm tauri build` 全绿 |

每步可独立 commit。

---

## 9. 风险点与决策记录

### 9.1 已记录决策（brainstorming 收敛结果）

| 决策点 | 结论 | 理由 |
|---|---|---|
| 是否多 tab | 否 | "转换结果"区充当草稿，单次覆盖够用 |
| "采用"按钮 | 移除 | "不满意"通过编辑"转换结果"区表达；按钮多余 |
| 预览生成后是否自动落到"转换结果" | 否，需手动"↑ 复制" | 区分"预览"与"采用"两个区，分工明确 |
| seed 是否必填 | 否 | 不卡用户；seed=None 时首章走 LLM 队列 |
| `PreviewFirstChapter` 是否改名 `FirstChapterSeed` | 改名 | 语义不再是"preview"，是"首章 seed"；波及面可控 |
| IPC 字段名 `preview_first_chapter` 是否改名 | 保留 | 跟 Tauri 命令 DTO 历史命名挂钩，改名波及面过大 |
| preview 是否落 `chapter_previews` 表 | 否（dialog 内不落） | 沿用 `2026-08-20` §3.3 决定 |

### 9.2 已知风险

1. **首次"↑ 复制"已有 seedContent 时的 confirm**：沿用 `RegeneratePreviewDialog.onUsePreview` 已有的 confirm 模式（用户已验证）。**低风险**
2. **seed=None 时旧 batch 行为**：靠 `first_chapter_seed_does_not_overwrite_old_done_tc` 回归测试保证。**低风险**
3. **没有"采用"按钮后用户"忘记点复制"的认知风险**：用户在 brainstorming 中明确接受"用户自己负责"原则。**已接受**
4. **dialog 关闭再打开时 seed 丢失**：用户可能误关 dialog 后丢失手写内容。**低风险**（沿用现状：dialog 关闭即丢）

### 9.3 不在本次范围（backlog）

- 预览章节切换 UI（多 idx 预览）
- 多 tab 历史
- prompt 编辑器内预览
- ctx toggle 高级模式
- 首章 seed 之外的"种子链"（用户手写第二章 / 第三章）

---

## 10. 验证清单

完成后逐项确认：

- [ ] `cargo test -p nsc-core` 全绿
- [ ] `pnpm test` 全绿
- [ ] `pnpm tauri build --bundles msi` 通过
- [ ] 手动测试场景：
  - 打开 dialog → 选章节 / prompt / model → 直接点"创建"（不调预览）→ 工作流创建成功，首章作为 pending 等候 LLM
  - 打开 dialog → 选章节 / prompt / model → 点"生成预览" → 点"↑ 复制" → 点"创建" → 工作流创建成功，首章 status=done，tokens 正确
  - 打开 dialog → 选章节 / prompt / model → 在"转换结果"区手写 → 点"创建" → 工作流创建成功，首章 status=done，tokens_in/out=0
  - 打开 dialog → 生成预览 → 不复制 → 直接点"创建" → 与"无预览"同效（previewOutput 在 dialog 内丢弃）
  - 切换预览章节 → 三区全部清空（防串台）
  - 重选 prompt/model → seedContent 保留
  - "↑ 复制"在 previewOutput 为空时禁用
  - 重复"↑ 复制"在 seedContent 非空时弹 confirm（追加/替换）
- [ ] `RegeneratePreviewDialog` 行为不变
- [ ] `RegeneratePreviewDialog` 提交后 `chapter_previews` 清空逻辑不受影响

---

## 11. 状态跟踪

- brainstorming：✅ 收敛
- spec 审：⏳ 待用户阅读
- 计划：⏳ 待 writing-plans
- 实施：⏳
- 验证：⏳