# Prompt 管理 UI — 设计 spec

**日期**:2026-08-03
**作者**:brainstorming 阶段产物
**状态**:已批准(待 writing-plans → 实施)
**关联 backlog**:转换工作流(batch / frontier / serial / auto_continue)、压缩+文风组合 prompt、全本自动逐章转换

---

## 1. 范围

只做 prompt CRUD UI。其他转换相关工作流(批次概念、frontier 取最近已转换、batch 串行调度、auto_continue)单独 spec,本 spec 不涉及。

**In scope**:
- 列表、新建、编辑、删除、复制 builtin
- 删除前的引用计数确认

**Out of scope(backlog)**:
- 模板变量点击插入、双栏 live preview
- 批量删除、kind 过滤、import/export
- 与 TransformDialog 的集成(下一轮用同一 store,本轮不做)

---

## 2. 用户故事

1. 作为用户,我能在 `/prompts` 看到所有 prompt(builtin + 用户),按 builtin 在上、用户在下排列。
2. 我能从顶部"新建 prompt"按钮开 dialog,填 name / kind / template 后保存。
3. 我能在 builtin 行点"复制",弹 dialog 预填 name='<原>_copy' + 原 template,保存即生成可编辑的用户 prompt。
4. 我能在用户行点"编辑"改任意字段(builtin 不允许编辑)。
5. 我能在用户行点"删除",弹 confirm 显示"被 N 个转换结果引用",N > 0 时仍允许删除(只是告知,history 行的 `prompt_id` 变成孤儿,UI 仍能按 id 读到 history)。
6. template 未引用 `{{chapter_content}}` 时,dialog 底部黄字提示"该 prompt 未引用 {{chapter_content}},LLM 将无法看到章节正文",允许提交。

---

## 3. 架构

### 3.1 路由与导航

- 新路由:`src/router/index.ts` 加 `{ path: '/prompts', component: () => import('../views/Prompts.vue') }`
- `src/components/Sidebar.vue` 加一项 `prompts: { label: '提示词', icon: <PromptIcon> }`,与 Library / Models / Transform 平级
- 视图挂载时调 `store.load()`,与现有 `Library.vue` 同款模式

### 3.2 组件树

```
Prompts.vue (list + table + 新建按钮 + 删除 confirm)
└── PromptEditDialog.vue (新建 / 编辑 / 复制 builtin 三种模式共用)
    └── Dialog.vue (现有)
```

复用现有 `Dialog.vue` / `Button.vue` / `Input.vue`,不引入新基础组件。

### 3.3 状态层(Pinia store)

`src/stores/prompts.ts`,对齐现有 `useLibraryStore` / `useModelsStore` 风格:

| 字段 | 类型 | 说明 |
|------|------|------|
| `prompts` | `Prompt[]` | 列表,builtin 在前(id ASC),用户在后(id DESC) |
| `loading` | `boolean` | 加载中 |
| `error` | `string \| null` | 错误信息 |

Actions:

| 方法 | 行为 |
|------|------|
| `load()` | `invoke('list_prompts')` → 写入 `prompts` |
| `upsert(payload: PromptInput)` | `invoke('upsert_prompt', { payload })`,完成后调 `load()` 刷新 |
| `remove(id: number)` | `invoke('delete_prompt', { id })`,完成后调 `load()` 刷新 |
| `countUsage(id: number): Promise<number>` | `invoke('count_transformation_chapters_by_prompt', { promptId: id })`,返回数字 |

写后 `load()` 比 diff 简单,prompt 表小(典型 < 20 条),成本可忽略。

### 3.4 数据流

新建 prompt 流程:
```
Prompts.vue ─[click 新建]─▶ PromptEditDialog (mode='create')
                            ─[submit]─▶ store.upsert(payload)
                                       ─[invoke]─▶ 后端 upsert_prompt
                                       ─[reload]─▶ store.load() 刷新列表
```

删除 prompt 流程:
```
Prompts.vue ─[click 删除]─▶ 二次确认 Dialog
                            ─[confirm]─▶ store.countUsage(id) ─▶ 显示 N
                                       ─[确认]─▶ store.remove(id)
```

---

## 4. IPC 接口

新增 5 个命令,文件 `src-tauri/src/commands/prompts.rs`,在 `src-tauri/src/lib.rs::invoke_handler!` 注册。

### 4.1 命令清单

| 命令 | 入参(外层 camelCase) | 内层 DTO / 字段 | 返回 |
|------|---------------------|-----------------|------|
| `list_prompts` | — | — | `Vec<Prompt>` |
| `get_prompt` | `id: i64` | — | `Prompt` |
| `upsert_prompt` | `payload: PromptInput` | `PromptInput { id, name, kind, template }` | `i64`(新 / 改后 id) |
| `delete_prompt` | `id: i64` | — | `()` |
| `count_transformation_chapters_by_prompt` | `promptId: i64` | — | `i64` |

### 4.2 PromptInput DTO

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PromptInput {
    pub id: i64,           // 0 = 新建,>0 = 更新
    pub name: String,
    pub kind: PromptKind,
    pub template: String,
}
```

`Prompt` 直接复用 `nsc_core::models::Prompt`(`#[derive(Serialize)]` 已 ready)。

### 4.3 Frontend IPC wrapper

`src/ipc/commands.ts` 新增 5 个函数:

```ts
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

---

## 5. Backend 改动

### 5.1 `PromptRepo::count_by_prompt`

文件 `crates/nsc-core/src/db/repo/prompt.rs`,新增:

```rust
pub fn count_by_prompt(&self, prompt_id: i64) -> Result<i64> {
    let n: i64 = self.conn.query_row(
        "SELECT COUNT(*) FROM transformation_chapters WHERE prompt_id = ?1",
        params![prompt_id],
        |r| r.get(0),
    )?;
    Ok(n)
}
```

不动 schema,不动 `PromptRepo` 其他方法。

### 5.2 Tauri command 实现要点

- 所有命令拿 `State<'_, Arc<Mutex<Db>>>`,lock 后操作;`Error → String` 透传
- `upsert_prompt` 分支:`payload.id == 0` 走 `PromptRepo::insert(&Prompt { id: 0, ... })`(`Prompt` 已有所有字段),否则走 `update`。返回值是新插入 / 已更新的 id
- `delete_prompt` 不查引用计数(由前端 `countPromptUsage` 拿,后端只负责删)

---

## 6. Frontend 改动

### 6.1 类型

`src/ipc/types.ts` 新增:

```ts
export interface Prompt {
  id: number;
  name: string;
  kind: 'compress' | 'style';
  template: string;
  is_builtin: boolean;
}
export type PromptInput = Omit<Prompt, 'id' | 'is_builtin'> & { id: number };
```

### 6.2 `src/views/Prompts.vue`

模板结构:
- 顶 `<header>`:`<h2>提示词</h2>` + `<Button kind="primary">新建 prompt</Button>`
- 空状态:`还没有 prompt` —— 实际不会触发(永远有 2 个 builtin),保留用于防御
- 表格 `<Table>`:列 `name / kind / builtin 标记 / 操作`
- 操作列按钮:
  - builtin 行:`编辑`(disabled) / `删除`(disabled) / `复制`
  - 用户行:`编辑` / `删除`
- `<PromptEditDialog v-model:open="dialogOpen" :mode="..." :initial="..." @saved="onSaved" />`
- `<Dialog v-model:open="confirmOpen" title="删除 prompt">` 二次确认,内含 `await countPromptUsage` 拿到 N 后显示

### 6.3 `src/components/PromptEditDialog.vue`

Props:
- `mode: 'create' | 'edit' | 'copy-from-builtin'`
- `initial?: Prompt` —— 编辑 / 复制时带入

本地 state:
- `name: ref('')`(trim 后非空)
- `kind: ref<'compress' | 'style'>('compress')`
- `template: ref('')`
- `submitting: ref(false)`
- `error: ref<string | null>(null)`

派生:
- `isBuiltin = mode === 'edit' && initial?.is_builtin === true`(此模式下整个 dialog 全部字段 disabled —— 实际上 `mode='edit'` 不允许 builtin,故该分支为防御)
- `canSubmit = computed(() => name.value.trim() !== '' && template.value.trim() !== '' && !submitting.value)`
- `missingChapterContent = computed(() => !template.value.includes('{{chapter_content}}'))`
- 底部黄字提示:`<div v-if="missingChapterContent" class="warn">该 prompt 未引用 {{ '{{chapter_content}}' }},LLM 将无法看到章节正文</div>`

行为:
- `mode='create'`:空白表单
- `mode='edit'`:用 `initial` 填充
- `mode='copy-from-builtin'`:用 `initial` 填充,`name` 自动追加 `_copy`(只在名称未以 `_copy` 结尾时)
- 提交:`store.upsert({ id: mode === 'create' ? 0 : initial!.id, name: name.value.trim(), kind, template: template.value })`
- 成功后 `emit('saved')`,父组件刷新列表并关 dialog

### 6.4 Sidebar 项

`src/components/Sidebar.vue` 在 Library / Models / Transform 之间加一项:

```ts
{ name: 'prompts', label: '提示词', to: '/prompts' }
```

---

## 7. 校验与失败处理

| 场景 | 行为 |
|------|------|
| name / template trim 后为空 | submit 按钮 disabled,无需弹错 |
| builtin prompt 编辑 | 整个 dialog 字段 disabled(防御,实际上 UI 不应进入此模式) |
| builtin prompt 删除 | 按钮 disabled |
| 删除时被引用 | confirm 文案加"被 N 个转换结果引用",N=0 时不显示 |
| 删除失败 | `alert(e.message)`(沿用 Library 的错误处理风格) |
| 后端 upsert 失败 | dialog 内 `error.value = e.message`,不关 dialog |
| 加载失败 | 列表上方红色 alert,沿用 Library 风格 |

---

## 8. 测试策略

### 8.1 Backend

新增 / 修改:

- `crates/nsc-core/tests/db_prompt.rs` 新增测试 `count_by_prompt_returns_ref_count`:
  - seed 1 个 builtin,插入 3 条 `transformation_chapters` 用它,调 `count_by_prompt`,断言 = 3
  - 再插 2 条用另一个 prompt,断言旧的仍 = 3
- `crates/nsc-core/tests/db_prompt.rs` 新增测试 `count_by_prompt_zero_for_unused`:
  - 建一个 prompt,调 `count_by_prompt`,断言 = 0

不动已有 db_prompt / queue_provider / transformations 测试。

### 8.2 Frontend

新增 `src/__tests__/prompts.spec.ts`(对齐 `library.spec.ts` / `models.spec.ts` 风格):

- `list / upsert / delete / countUsage` 4 个 wrapper 的 invoke 调用形状断言(确认 camelCase outer + snake_case inner DTO)
- `store.load()` 后 `prompts` 被填充
- `store.upsert({ id: 0, ... })` → invoke 收到 `payload`,然后 `load()` 被再调一次
- `store.upsert({ id: 7, ... })` 走 update 分支
- `store.remove(7)` → invoke 收到 `{ id: 7 }`,然后 `load()` 被再调一次
- `store.countUsage(7)` 返回 Promise<number>

不写组件树 / E2E(vitest happy-dom + mock invoke 已经覆盖 IPC 形状,组件树渲染测试本项目不写,沿用现有惯例)。

---

## 9. 不做(deferred backlog)

- 变量点击插入、双栏 live preview
- 批量删除、kind 过滤、搜索、import/export
- 与 TransformDialog 的集成(下一轮转换工作流 spec 一起做)
- E2E 覆盖(playwright `test.skip` 占位仍是占位)
- `mpsc::unbounded` queue 限流、cancel / retry UI

---

## 10. 验收标准

1. `pnpm test` 100% 通过(99 + 新增 ~6 个)
2. `cargo test -p nsc-core` 100% 通过(已有 + 新增 ~2 个)
3. `cargo build --workspace` 无 warning
4. 手工跑:`pnpm tauri dev`,访问 `/prompts`,能看到 2 个 builtin + 可新建 / 编辑 / 删除 / 复制
5. 删除一个有 history 引用计数的 prompt,confirm 弹"被 N 个转换结果引用",确认后删成功,刷新列表不再显示
6. 未引用 `{{chapter_content}}` 的 prompt,dialog 底部显示黄字警告