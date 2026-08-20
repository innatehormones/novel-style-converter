# 「新建工作流」试运行区 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在「新建工作流」对话框加入试运行区，让用户在创建工作流前预览首章，避免带着有问题的 prompt / 前文配置浪费 token。

**Architecture:**
- 后端新增 IPC 命令 `preview_first_chapter` 调一次 `transform_with_business(req, RegeneratePreview)`，不触发 batch / tc 写入
- 后端 `create_workflow` 入参扩展 `preview_first_chapter: Option<...>`，事务内把 idx 最小那个 chapter 对应的 tc 标 done
- 前端 `CreateBatchDialog.vue` 改左右分栏，右下半试运行区，复用现有 toggle / button 组件

**Tech Stack:**
- Backend: Rust + rusqlite + tokio（已有）
- Frontend: Vue 3 + TypeScript + Pinia（已有）
- AI 调用日志：复用 `AiCallBusiness::RegeneratePreview`

**Spec:** `docs/superpowers/specs/2026-08-20-create-workflow-preview-design.md`

---

## File Structure

### 后端新增
- `crates/nsc-core/src/models/transformation.rs` — 加 `PreviewFirstChapter` struct
- `crates/nsc-core/src/transformer/batch_scheduler.rs` — `WorkflowCreate` 加字段；create_workflow 加 preview 路径；新增单测
- `src-tauri/src/commands/workflows.rs` — 加 `PreviewFirstChapterInput/Output` + `preview_first_chapter` tauri 命令
- `src-tauri/src/lib.rs` — 注册命令

### 前端新增 / 改
- `src/ipc/types.ts` — 加 `PreviewFirstChapterInput/Output` + `CreateWorkflowInput.preview_first_chapter`
- `src/ipc/commands.ts` — 加 `previewFirstChapter()` wrapper
- `src/components/CreateBatchDialog.vue` — 大改：左右分栏 + ctx toggle + 试运行区

### 文档
- `docs/optimization-notes.md` — 加业务流转章节

---

## Task 1: 后端 - 加 `PreviewFirstChapter` 模型

**Files:**
- Modify: `crates/nsc-core/src/models/transformation.rs` (在 `TransformationNovel` / `Batch` struct 附近)

- [ ] **Step 1: 在 transformation.rs 末尾加 struct**

```rust
/// 用户在「新建工作流」试运行区满意的首章结果,作为创建工作流时的 seed。
/// 后端事务内把 idx 最小那个 chapter 对应的 tc 标 done + 写 result_content。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewFirstChapter {
    pub content: String,
    pub tokens_in: i32,
    pub tokens_out: i32,
}
```

- [ ] **Step 2: 跑 `cargo check` 验证编译**

Run: `cd D:\Git\novel-style-converter && cargo check --package nsc-core`
Expected: 编译通过(无 warning)

- [ ] **Step 3: Commit**

```bash
cd D:\Git\novel-style-converter
git add crates/nsc-core/src/models/transformation.rs
git commit -m "feat(core): 加 PreviewFirstChapter 模型"
```

---

## Task 2: 后端 - `WorkflowCreate` 加 `preview_first_chapter` 字段

**Files:**
- Modify: `crates/nsc-core/src/transformer/batch_scheduler.rs:48-60` (`WorkflowCreate` struct)

- [ ] **Step 1: 修改 struct 定义**

在 `WorkflowCreate` 末尾 (`on_failure_policy` 后) 加字段:

```rust
    pub on_failure_policy: OnFailurePolicy,
    /// 试运行首章结果(由「新建工作流」对话框传入)。Some → 事务内把 idx 最小那个
    /// chapter 对应的 tc 标 done;None → 全部 tc pending(原行为)。
    pub preview_first_chapter: Option<crate::models::PreviewFirstChapter>,
}
```

- [ ] **Step 2: 跑 `cargo check` 验证编译**

Run: `cargo check --package nsc-core`
Expected: 编译通过,可能提示 `WorkflowCreate` 构造点缺字段 → 看 Step 3

- [ ] **Step 3: 修复所有构造点**

```bash
cd D:\Git\novel-style-converter
rg -n "WorkflowCreate {" crates/nsc-core/src --type rust
```

对每个匹配的文件,在构造点末尾加 `preview_first_chapter: None,`。

- [ ] **Step 4: 跑测试确认现有逻辑不变**

Run: `cargo test --package nsc-core --lib`
Expected: 39 passed;0 failed

- [ ] **Step 5: Commit**

```bash
git add crates/nsc-core/src/transformer/batch_scheduler.rs <其他构造点文件>
git commit -m "feat(core): WorkflowCreate 加 preview_first_chapter 字段(default None)"
```

---

## Task 3: 后端 - `BatchScheduler::create_workflow` 加 preview 路径 + 单测

**Files:**
- Modify: `crates/nsc-core/src/transformer/batch_scheduler.rs` (`create_workflow` 事务体,约 line 88-180)
- Test: `crates/nsc-core/src/transformer/batch_scheduler.rs` 末尾 `mod tests`

- [x] **Step 1: 看 `create_workflow` 当前事务结构**

Read file 88-180,理解 INSERT batch / INSERT tc / INSERT wr / INSERT wrc 的顺序。

- [x] **Step 2: 写 failing test** (调整:`create_workflow` 派发副作用难隔离,改为测独立的 `apply_preview_in_tx` 函数,直接构建事务测试 seed)


在文件末尾 `mod tests` 加:

```rust
    #[test]
    fn create_workflow_with_preview_seeds_first_chapter_done() {
        // 用现有 fresh_db / helper 建一个 tn + 3 个 chapter
        let dir = tempfile::tempdir().unwrap();
        let db = nsc_core::db::Db::open(dir.path().join("test.db")).unwrap();
        // seed: 1 upload + 1 data_asset + 3 chapter (idx 0..2) + 1 tn
        // 参考已有 test 的 seed 模式,不要重新发明
        let sched = BatchScheduler::new(db.clone());
        let preview = PreviewFirstChapter {
            content: "preview result".into(),
            tokens_in: 100,
            tokens_out: 200,
        };
        let spec = WorkflowCreate {
            tn_id: tn_id,
            label: Some("test".into()),
            chapter_ids: vec![c0, c1, c2],
            prompt_id,
            model_config_id,
            mode: PromptKind::Style,
            ctx_prev_original: 0,
            ctx_prev_transformed: 0,
            ctx_next_original: 0,
            on_failure_policy: OnFailurePolicy::PauseAndReview,
            preview_first_chapter: Some(preview.clone()),
        };
        let batch = sched.create_workflow(spec).unwrap();
        let tcs = db.transformation_chapters().list_by_batch(batch.id).unwrap();
        let tc0 = tcs.iter().find(|t| t.chapter_id == c0).unwrap();
        let tc1 = tcs.iter().find(|t| t.chapter_id == c1).unwrap();
        assert_eq!(tc0.status, TransformStatus::Done);
        assert_eq!(tc0.result_content.as_deref(), Some("preview result"));
        assert_eq!(tc0.tokens_in, Some(100));
        assert_eq!(tc0.tokens_out, Some(200));
        assert_eq!(tc1.status, TransformStatus::Pending);
        // wrc 也写了 idx=0 的内容
        let wrc = db.workflow_result_chapters().get(batch.id, c0).unwrap().unwrap();
        assert_eq!(wrc.content, "preview result");
    }

    #[test]
    fn create_workflow_without_preview_keeps_all_pending() {
        // 同上但 preview_first_chapter: None
        // 断言所有 tc 都是 Pending,wrc 没有 c0 行
    }
```

注:具体 seed 细节参考 `crates/nsc-core/src/transformer/batch_scheduler.rs` 已有的 `mod tests` —— 用 `fresh_db` / `seed_*` helper,不要重新发明。

- [x] **Step 3: 跑测试,确认 FAIL**

Confirmed: apply_preview UPDATE 返回 0 rows(参数顺序错:batch_id 占位 ?5 但塞在 params 第 1 位 — SQL 占位符 ?5 实际拿到了 now 字符串)。

- [x] **Step 4: 实现 preview 路径**


在 `create_workflow` 事务内,**INSERT 所有 tc + wrc 之后**,如果 `spec.preview_first_chapter.is_some()`:

1. 找 idx 最小的 chapter_id (`SELECT MIN(idx) FROM chapters WHERE id IN (chapter_ids)`)
2. UPDATE 对应 tc: `status='done'`, `result_content=content`, `tokens_in/out`, `completed_at=now`
3. UPDATE wrc: `content=content`, `updated_at=now` (`WHERE batch_id=? AND chapter_id=?`)

参考已有 SQL 模式,在事务内用 `tx.execute(...)`。

- [x] **Step 5: 跑测试,确认 PASS**

`./target/debug/deps/nsc_core-*.exe --test-threads=1`: 41 passed (39 + 2 新)。

- [x] **Step 6: 跑 clippy 看新警告**

修了 doc list 缩进 + `repeat_n` 两处。`batch_scheduler.rs` 0 warning(其他预存在的 promotion.rs warning 与本任务无关)。

- [ ] **Step 7: Commit**

```bash
git add crates/nsc-core/src/transformer/batch_scheduler.rs crates/nsc-core/tests/_tmp_repro.rs
git commit -m "feat(core): create_workflow 加 preview_first_chapter 路径"
```

注:`tests/_tmp_repro.rs` 是 Task 2 引入的临时文件,本次一并删除。

---

## Task 4: 后端 - `preview_first_chapter` IPC 命令

**Files:**
- Modify: `src-tauri/src/commands/workflows.rs` (末尾加新 tauri command)
- Modify: `src-tauri/src/lib.rs:150-160` (`invoke_handler` 注册)

- [ ] **Step 1: 看现有 commands/workflows.rs 的命令模式**

Read first 80 lines,理解 `list_workflows` / `create_workflow` 的 DTO 转换模式。

- [ ] **Step 2: 加 DTO + 命令**

在 `commands/workflows.rs` 末尾加:

```rust
use nsc_core::models::PromptKind;
use nsc_core::transformer::DefaultTransformer;

/// `preview_first_chapter` 入参(IPC 边界 snake_case)。
#[derive(Debug, Deserialize)]
pub struct PreviewFirstChapterInput {
    pub tn_id: i64,
    pub chapter_id: i64,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub include_prev: bool,
    pub include_next: bool,
    pub custom_input: Option<String>,
}

/// `preview_first_chapter` 出参。
#[derive(Debug, Serialize)]
pub struct PreviewFirstChapterOutput {
    pub content: String,
    pub tokens_in: i32,
    pub tokens_out: i32,
}

#[tauri::command]
pub async fn preview_first_chapter(
    db: State<'_, Arc<Db>>,
    input: PreviewFirstChapterInput,
) -> Result<PreviewFirstChapterOutput, String> {
    // 1. 读 chapter + chapter_content
    let chapter = db.chapters().get(input.chapter_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("chapter {} 不存在", input.chapter_id))?;
    let content = chapter.body.clone();
    // 2. 读 prompt + model_config(未归档)
    let prompt = db.prompts().get(input.prompt_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("prompt {} 不存在", input.prompt_id))?;
    let model = db.model_configs().get(input.model_config_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("model_config {} 不存在", input.model_config_id))?;
    if model.archived != 0 {
        return Err(format!("model_config {} 已归档", input.model_config_id));
    }
    // 3. 读 tn 拿 mode
    let tn = db.transformation_novels().get(input.tn_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("tn {} 不存在", input.tn_id))?;
    // 4. 组装 PrevContext / NextContext
    let prev_original = if input.include_prev {
        db.chapters().prev_n(chapter.data_asset_id, chapter.idx, 1).unwrap_or_default()
    } else { String::new() };
    let prev_transformed = String::new();
    let next_original = if input.include_next {
        db.chapters().next_n(chapter.data_asset_id, chapter.idx, 1).unwrap_or_default()
    } else { String::new() };
    // 5. 调 transformer
    let db_for_transform = db.inner().clone();
    let req = TransformRequest {
        novel_context: TransformationNovelContext {
            transformation_novel: tn,
            prev_original,
            prev_transformed,
            next_original,
        },
        chapter: chapter.clone(),
        chapter_content: content,
        prompt,
        model_config: model,
        custom_input: input.custom_input,
    };
    let outcome = DefaultTransformer::new(db_for_transform)
        .transform_with_business(req, AiCallBusiness::RegeneratePreview)
        .await
        .map_err(|e| e.to_string())?;
    Ok(PreviewFirstChapterOutput {
        content: outcome.content,
        tokens_in: outcome.tokens_in,
        tokens_out: outcome.tokens_out,
    })
}
```

注:
- `DefaultTransformer::new` / `transform_with_business` / `TransformRequest` / `TransformationNovelContext` 已在现有代码,直接用
- `db.chapters().prev_n` / `next_n` 已有(看 `crates/nsc-core/src/db/repo/chapter.rs`)
- 若 `prev_n` / `next_n` 签名不同,调整为现有签名

- [ ] **Step 3: 注册命令**

在 `src-tauri/src/lib.rs` 的 `invoke_handler` builder 加 `commands::workflows::preview_first_chapter,`。

- [ ] **Step 4: 跑编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过(可能需要微调 type / import)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/workflows.rs src-tauri/src/lib.rs
git commit -m "feat(commands): 加 preview_first_chapter IPC 命令"
```

---

## Task 5: 前端 - 类型 + IPC wrapper

**Files:**
- Modify: `src/ipc/types.ts` (在 `CreateWorkflowInput` 后加 preview 类型)
- Modify: `src/ipc/commands.ts` (末尾加 wrapper)

- [ ] **Step 1: 加类型**

`src/ipc/types.ts` 末尾:

```typescript
/** `preview_first_chapter` 入参。 */
export interface PreviewFirstChapterInput {
  tn_id: number;
  chapter_id: number;
  prompt_id: number;
  model_config_id: number;
  include_prev: boolean;
  include_next: boolean;
  custom_input: string | null;
}

/** `preview_first_chapter` 出参。 */
export interface PreviewFirstChapterOutput {
  content: string;
  tokens_in: number;
  tokens_out: number;
}

/** 试运行首章结果(用户满意后传入 create_workflow)。 */
export interface PreviewFirstChapter {
  content: string;
  tokens_in: number;
  tokens_out: number;
}
```

- [ ] **Step 2: 给 CreateWorkflowInput 加 preview_first_chapter 字段**

```typescript
export interface CreateWorkflowInput {
  // ... 现有字段
  preview_first_chapter: PreviewFirstChapter | null;
}
```

- [ ] **Step 3: 加 IPC wrapper**

`src/ipc/commands.ts` 末尾:

```typescript
export function previewFirstChapter(
  input: PreviewFirstChapterInput,
): Promise<PreviewFirstChapterOutput> {
  return invoke<PreviewFirstChapterOutput>('preview_first_chapter', { input });
}
```

- [ ] **Step 4: 跑 vue-tsc**

Run: `cd src && pnpm vue-tsc --noEmit`
Expected: 0 error

- [ ] **Step 5: Commit**

```bash
git add src/ipc/types.ts src/ipc/commands.ts
git commit -m "feat(ipc): 加 previewFirstChapter 类型 + wrapper"
```

---

## Task 6: 前端 - CreateBatchDialog 左右分栏 + ctx toggle

**Files:**
- Modify: `src/components/CreateBatchDialog.vue`

- [ ] **Step 1: 备份现状,理清要改的字段**

`ref` 状态清单:
- `ctxPrevOriginal` / `ctxPrevTransformed` / `ctxNextOriginal` → 删除,改为 `includePrev: bool` / `includeNext: bool`
- 新增 `previewFirstChapter: PreviewFirstChapter | null` (用户满意缓存)
- 新增 preview 状态: `previewLoading`, `previewError`, `previewOriginalText` (第一次进 dialog 时读)

- [ ] **Step 2: Dialog width 540 → 880**

`Dialog v-model:open="open" title="新建工作流" :width="880"`

- [ ] **Step 3: 改成 grid 2 列布局**

包裹内容在 `<div class="dialog-grid">`,CSS:

```css
.dialog-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
  min-height: 400px;
}
.dialog-grid > .left-col,
.dialog-grid > .right-col {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
```

- [ ] **Step 4: 把原 3 个 ctx NumberInput 替换为 2 个 toggle**

```html
<div class="row ctx-toggles">
  <label class="toggle">
    <input type="checkbox" v-model="includePrev" />
    <span>带前文</span>
  </label>
  <label class="toggle">
    <input type="checkbox" v-model="includeNext" />
    <span>带后文</span>
  </label>
</div>
```

删除 `.ctx-hint` 段。

- [ ] **Step 5: 移除 NumberInput import**

如果 toggle 替换后不再用,清理 import。

- [ ] **Step 6: 跑 vue-tsc**

Run: `cd src && pnpm vue-tsc --noEmit`
Expected: 0 error

- [ ] **Step 7: 启动 dev 肉眼检查布局**

Run: `npm run tauri dev`
- 打开任一转换工程的「新建工作流」
- 确认左右分栏、ctx toggle 位置正确
- 还没加试运行区,先 commit 这一阶段

- [ ] **Step 8: Commit**

```bash
git add src/components/CreateBatchDialog.vue
git commit -m "refactor(ui): CreateBatchDialog 改左右分栏 + ctx 改为 toggle"
```

---

## Task 7: 前端 - 试运行区 UI + state 管理

**Files:**
- Modify: `src/components/CreateBatchDialog.vue` (template 右下半 + script 状态)
- Modify: `src/views/TransformationNovelDetail.vue` (传 selectedChapterIds 的最小 idx 对应 chapter_id 给 dialog)

- [ ] **Step 1: 加 preview 状态 + computed**

`script setup` 加:

```typescript
import { ref, computed, watch } from 'vue';
import { previewFirstChapter } from '../ipc/commands';
import type { PreviewFirstChapter } from '../ipc/types';

const includePrev = ref(false);
const includeNext = ref(false);
const previewFirstChapterRef = ref<PreviewFirstChapter | null>(null);
const previewLoading = ref(false);
const previewError = ref<string | null>(null);
const previewOriginal = ref('');
const previewOutput = ref('');
const previewAccepted = ref(false);

// 「生成预览」按钮可用的条件:有预览章节 + 选了 prompt/model
const canPreview = computed(() =>
  props.previewChapterId !== null &&
  promptId.value !== 0 &&
  modelConfigId.value !== 0 &&
  !previewLoading.value,
);
```

- [ ] **Step 2: 加 props.previewChapterId**

`props` 加 `previewChapterId: number | null`,dialog 接收父组件传值。

- [ ] **Step 3: watch previewChapterId 加载原文**

```typescript
watch(() => props.previewChapterId, async (id) => {
  if (id === null) {
    previewOriginal.value = '';
    return;
  }
  try {
    const ch = await getChapter(id);
    previewOriginal.value = ch.body;
  } catch (e: unknown) {
    previewError.value = e instanceof Error ? e.message : String(e);
  }
});
```

`getChapter` 命令若不存在,从 `list_chapters` 或类似接口读;看 `src/ipc/commands.ts` 已有命令选择。

- [ ] **Step 4: 加「生成预览」按钮 + 调用函数**

```typescript
async function onGeneratePreview() {
  if (props.previewChapterId === null) return;
  if (promptId.value === 0 || modelConfigId.value === 0) {
    previewError.value = '请先选择 prompt 和 model';
    return;
  }
  previewLoading.value = true;
  previewError.value = null;
  try {
    const out = await previewFirstChapter({
      tn_id: props.tnId,
      chapter_id: props.previewChapterId,
      prompt_id: promptId.value,
      model_config_id: modelConfigId.value,
      include_prev: includePrev.value,
      include_next: includeNext.value,
      custom_input: null,
    });
    previewOutput.value = out.content;
    previewFirstChapterRef.value = out;
    previewAccepted.value = false;
  } catch (e: unknown) {
    previewError.value = e instanceof Error ? e.message : String(e);
    previewOutput.value = '';
  } finally {
    previewLoading.value = false;
  }
}
```

- [ ] **Step 5: 加「满意,使用此结果」按钮**

```typescript
function onAcceptPreview() {
  if (!previewFirstChapterRef.value) return;
  previewAccepted.value = true;
}
```

- [ ] **Step 6: 加 UI 区块(template)**

在右列下半:

```html
<div class="preview-pane">
  <div class="preview-header">
    预览章节 <span v-if="props.previewChapterId">#{{ props.previewChapterId }}</span>
  </div>
  <label class="preview-label">原文</label>
  <textarea
    class="preview-original"
    :value="previewOriginal"
    readonly
    rows="6"
  ></textarea>
  <div class="preview-actions">
    <Button @click="onGeneratePreview" :loading="previewLoading" :disabled="!canPreview">
      {{ previewOutput ? '重新生成' : '生成预览' }}
    </Button>
    <Button
      v-if="previewOutput && !previewAccepted"
      kind="primary"
      @click="onAcceptPreview"
    >满意,使用此结果</Button>
    <Button
      v-else-if="previewAccepted"
      kind="primary"
      @click="previewAccepted = false; previewFirstChapterRef = null"
    >已选 ✓ 重新选</Button>
  </div>
  <div v-if="previewError" class="preview-error">{{ previewError }}</div>
  <label class="preview-label">转换预览</label>
  <textarea
    class="preview-output"
    :value="previewOutput"
    readonly
    rows="10"
  ></textarea>
</div>
```

- [ ] **Step 7: 加 CSS**

```css
.preview-pane { display: flex; flex-direction: column; gap: 8px; }
.preview-header { font-size: 13px; color: var(--text-secondary); }
.preview-label { font-size: 12px; color: var(--text-muted); }
.preview-original,
.preview-output {
  font-family: var(--font-mono);
  font-size: 12px;
  padding: 8px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  background: var(--bg-section);
  resize: vertical;
}
.preview-actions { display: flex; gap: 8px; }
.preview-error { color: var(--danger); font-size: 12px; }
```

- [ ] **Step 8: 跑 vue-tsc**

Run: `cd src && pnpm vue-tsc --noEmit`
Expected: 0 error

- [ ] **Step 9: 启动 dev 肉眼检查**

Run: `npm run tauri dev`
- 打开「新建工作流」
- 选 prompt / model → 点「生成预览」→ 看结果
- 「重新生成」多次 → 确认 ai_call_logs 页面多条 RegeneratePreview
- 「满意」按钮切换状态
- 关 dialog → 重新打开 → previewFirstChapterRef 应清空(watch reset)

- [ ] **Step 10: Commit**

```bash
git add src/components/CreateBatchDialog.vue src/views/TransformationNovelDetail.vue
git commit -m "feat(ui): CreateBatchDialog 加试运行区(原文 + 生成预览 + 重新生成 + 使用此结果)"
```

---

## Task 8: 前端 - 集成 create_workflow 提交 preview_first_chapter

**Files:**
- Modify: `src/components/CreateBatchDialog.vue` (`onSubmit` 函数 + `canSubmit` computed)
- Modify: `src/views/TransformationNovelDetail.vue` (`onCreateBatch` 处理 submit 入参带 preview)
- Modify: `src/stores/workflows.ts` (createWorkflow action 加 preview_first_chapter 字段,如需要)

- [ ] **Step 1: 修改 onSubmit payload**

`onSubmit` 里:

```typescript
emit("submit", {
  tn_id: props.tnId,
  label: label.value.trim(),
  chapter_ids: [...props.selectedChapterIds],
  prompt_id: promptId.value,
  model_config_id: modelConfigId.value,
  mode,
  ctx_prev_original: includePrev.value ? 1 : 0,
  ctx_prev_transformed: includePrev.value ? 1 : 0,
  ctx_next_original: includeNext.value ? 1 : 0,
  on_failure_policy: onFailurePolicy.value,
  preview_first_chapter: previewFirstChapterRef.value,
});
```

- [ ] **Step 2: 修改 canSubmit**

要求 previewFirstChapterRef 非空(用户必须满意):

```typescript
const canSubmit = computed(() =>
  promptId.value !== 0 &&
  modelConfigId.value !== 0 &&
  label.value.trim() !== "" &&
  props.selectedChapterIds.length > 0 &&
  previewFirstChapterRef.value !== null &&
  !submitting.value,
);
```

底部按钮加 title 提示「请先在右侧试运行区生成预览并满意」。

- [ ] **Step 3: 父组件 TransformationNovelDetail.vue 计算 previewChapterId**

传 dialog 时计算 previewChapterId = selectedChapterIds 中 idx 最小那个 chapter.id。具体 store / data 来源看 TransformationNovelDetail.vue 现有 code。

- [ ] **Step 4: 检查 ipc/commands.ts 的 createWorkflow wrapper**

确保 `createWorkflow(input: CreateWorkflowInput)` 透传 `preview_first_chapter` 字段。无需改实现(透传 snake_case 由 serde 自动)。

- [ ] **Step 5: 跑 vue-tsc + 测试**

Run: `cd src && pnpm vue-tsc --noEmit && cargo test --package nsc-core --lib`
Expected: 0 error,所有测试 pass

- [ ] **Step 6: 手动测试端到端**

Run: `npm run tauri dev`
1. 打开「新建工作流」
2. 选 prompt / model → 勾「带前文」/「带后文」→ 点「生成预览」
3. 「满意,使用此结果」
4. 点「创建」
5. 切到工作流详情:看 idx=0 done,idx=1..N-1 pending
6. 等候跑完 → promote → 数据资产正确

- [ ] **Step 7: Commit**

```bash
git add src/components/CreateBatchDialog.vue src/views/TransformationNovelDetail.vue src/ipc/ src/stores/
git commit -m "feat(ui): 集成 create_workflow preview_first_chapter 提交"
```

---

## Task 9: 文档 - 更新业务流转章节

**Files:**
- Modify: `docs/optimization-notes.md` (加「新建工作流」流程描述)

- [ ] **Step 1: 找到文档现有章节结构**

Read file 看现有结构。

- [ ] **Step 2: 加「试运行」章节**

写明:
- 用户视角流程(打开 dialog → 选 → 生成预览 → 满意 → 创建)
- 后端事务原子性(create_workflow 单事务完成 batch + tc + preview seed)
- ai_call_logs 业务类型:RegeneratePreview(预览) vs TransformChapter(正式)

- [ ] **Step 3: Commit**

```bash
git add docs/optimization-notes.md
git commit -m "docs: optimization-notes 加「新建工作流」试运行流程"
```

---

## Self-Review Checklist

实现后跑一遍:

- [ ] `cargo test --package nsc-core --lib` 全过
- [ ] `cargo clippy --package nsc-core --lib` 0 warning
- [ ] `cd src && pnpm vue-tsc --noEmit` 0 error
- [ ] 手动测端到端:创建工作流 → 工作流跑 → promote → 数据资产内容正确
- [ ] ai_call_logs 页面:多次预览调一次 + 正式跑 N 条都正确
- [ ] 对话关闭(未点创建):preview 结果丢弃,无副作用
