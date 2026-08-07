# novel-style-converter Business Description (post-refactor)

## Two independent data blocks

1. Upload: raw file with sha + filename + size + original_text (editable for cleaning).
2. DataAsset + Chapter: a parsed package. Chapters carry the actual text inline via chapters.body (no more byte-range slicing of the upload).

A single upload can produce many data assets, and the data assets survive the upload being deleted. The link data_assets.upload_id is informational only (no FK, no UNIQUE).

## Module status (snapshot 2026-08)

| 模块 | 状态 | 主要边界 / 关键 commit |
|---|---|---|
| Upload (上传) | ✅ 完成 | 解耦 upload / data_asset;chapter.body 自包含;删除 preview + 非 cascade。详细见 "Upload module (post-refactor)" |
| Model (LLM 配置) | ✅ 完成 | 软删 + 密钥必抹 + per-model 并发 + ProviderCache;`seed_default_model_from_env` 静默兜底移除。详细见 "Model refactor — completed" |
| Prompts (提示词) | ✅ 完成 | enum 合并 (`TransformMode` → `PromptKind`);render 去 Result;软删;`prev_transformed` 真接 `workflow_result_chapters.content` |
| AI call logs (调用日志) | ✅ 完成 + 收尾 | 表 + recorder + UI 看板;dead chain (`list_ai_call_logs_by_context`) 全删;`let _ = PromptKind::Compress` 遮掩代码删;**启动 panic 修复** —— `spawn_writer` 改 `std::thread::spawn` + 内建 tokio runtime |
| Workflows (转换 / batch) | ⏳ 计划已拍板,0 代码改动 | 见 "Workflows — 待重构" 章节;**实际页面操作未跑通**(用户自述) |

**核心架构原则**(贯穿所有已重构模块):

1. **不 silent 兜底**: env seed / 默认模型注入 / 兜底 provider 等"沉默补救"全部移除,改 fail-fast + UI 显式提示
2. **不静默丢数据**: 用户删除 X 但 Y 引用 X → 要么 cascade,要么软删 + UI 提示,**绝不**静默成功
3. **不遮掩问题**: `let _ = some_field` / `unwrap_or_else(... default ...)` / 注释里的 "暂时忽略" 都视为技术债,要么真用上,要么删
4. **upload / data_asset 解耦**: 上传是原始材料(mutable),数据资产是加工产物(chapter.body 自包含);删除 upload 不 cascade,孤儿 da 仍可访问
5. **跨模块引用靠软关系 + 反范式**: `data_assets.upload_id` / `ai_call_logs.model_config_id` / `ai_call_logs.context_*` 都是软引用(无 FK),被引用的对象被删不影响日志/数据可读
6. **业务写入路径走 repo + IPC**: 命令层不再持有 inline SQL;4 处 workflows 内联 SQL 待搬到 typed repos(下一轮重构)

## Optimization log

### Upload refactor — completed

- Uploads now represent only the original file and its editable source text; uploading does not implicitly create or replace a data asset.
- Chapter parsing can be committed repeatedly from the same upload, producing independent data assets.
- Committed chapters store their complete text in `chapters.body`; later upload cleaning, editing, or deletion does not invalidate existing data assets.
- Upload deletion is non-cascading. Before deletion, the UI previews derived data assets and only prompts the user to remove those assets manually from the data asset module.
- Data asset counts are visible in the upload list, and the parse entry remains available even when the upload already has derived data assets.
- The obsolete warning that cleaning would destroy existing chapter ranges was removed because chapter content is now self-contained.
- Parse-page state is isolated from the upload source after commit; leaving the page unloads large temporary text and chapter collections.

*(→ 详见 `## Upload module (post-refactor)`,含完整 UI / flow / design intent。本段记录 Upload 边界,作为 review 后续模块的基线)*
### Model (LLM 配置) refactor — completed

- 删除 `seed_default_model_from_env` 静默兜底:应用启动不再自动从 `NSC_DEFAULT_MODEL_*` 环境变量种入默认模型。所有模型必须由用户在模型管理页显式新增;空表首启时 UI 显式提示"尚未配置任何模型"。
- `model_configs.concurrency` 字段真正生效:worker 端每个 model 一个 `tokio::sync::Semaphore`,限流上限 = `model_config.concurrency`(`<= 0` 退化为 1)。
- `JobQueue` worker 内部新增 `ProviderCache`:按 `model_config_id` 缓存 `Arc<dyn AiProvider>`,cache miss 时通过 `provider_factory` 重建。
  - 替代旧的"每章重建一次 `reqwest::Client`"路径,避免反复 TLS 握手 / DNS 解析 / 连接池初始化。
  - cache key 用 `model_config.id`(不是 api_key / base_url):用户在 UI 改 key 后,运行中的 worker 仍持有旧 provider;新建工作流时新 key 才生效 —— 显式 trade-off,避免运行中突然 401。
  - cache 不在 worker 间共享,避免 `Arc<dyn AiProvider>` 跨线程引用计数竞争。
- `model_configs` 软删:`archived INTEGER NOT NULL DEFAULT 0` 列(migration v16)。
  - `delete_model` 是 `UPDATE … SET archived = 1, api_key = ''`:行保留,但**密钥必清空**(用户明确要求"密钥不能随归档泄露")。
  - `ModelConfigRepo::get(id)` 不按 archived 过滤:`BatchScheduler` / `transformation_chapters` 读 path 必须能拿到归档行,否则历史 tc 引用解析会断。
  - `list_models(include_archived: bool)`:`false` 仅返活动行(各 dialog 下拉用),`true` 含归档(Models.vue 顶部"显示已归档"切换)。
  - 新增 `restore_model` 命令:取消软删;注意 `api_key` 不会自动恢复(已被抹掉),用户需重新编辑保存。
- `test_model` 改返回结构化 `TestModelReport { model, base_url, latency_ms, tokens_in, tokens_out, content_preview, error }`:
  - **失败不再抛错**:错误字符串进 `report.error`,`latency_ms` 仍填(连 provider 创建失败也计超时)。
  - UI 完整展示:成功 / 失败标签 + latency + tokens in/out + 前 200 字 preview(或完整错误)。
  - `OpenAiProvider` 不动:latency 测度在 command 层包 `Instant::now()`。
- 前端:
  - `ModelConfig` 类型加 `archived: number`;`ModelConfigInput` 去掉 archived(不可通过 upsert 改)。
  - `Models.vue` 加"显示已归档"开关、归档行加"已归档"徽标 + 删除线 + 单一"恢复"按钮(替代"编辑/删除")。
  - `ModelDialog.vue` "测试连接"改读结构化 report,完整展示所有字段。
  - 空 Models 表显式说明"不会从环境变量自动注入默认模型"。
- 已知限制:
  - `concurrency` 超过物理 worker 数(默认 2)不会触发额外阻塞(没有那么多 worker 同时跑)。模型管理可设的上限是 16,但实际生效 = `min(concurrency, 物理并发)`。
  - 软删 model 后,如果新建同名 model,新行 id 增长;老归档行不会被覆盖,可恢复。
  - 软删行参与 `transformation_chapters.model_config_id` 引用解析时,`api_key=''` 仍可能被 `OpenAiProvider::new` 成功构造 —— 但实际发请求时会被远端 401,error 会冒到 transformation_chapters.mark_failed 路径(这是 fail-fast 行为,符合用户"不要遮掩问题"的要求)。
*(→ 详见 `## Model module (post-refactor)`,含 Data / UI / flow / design intent)*

## Model module (post-refactor)

### Data
- `model_configs` 表: name / model / base_url / api_key / temperature / max_tokens / **concurrency** / **archived** (0=活动,1=软删)
- `concurrency` 是 per-model 限流上限;worker 端按 `model_config_id` 建 `tokio::sync::Semaphore`
- 软删行 `api_key=""` (用户明确要求"密钥不能随归档泄露");恢复时密钥不会自动回来

### UI / flow

#### 1. Models.vue `/models`(列表)
- 列:name / model / base_url / temperature / max_tokens / **concurrency** / **状态** / 操作
- 顶部开关:**显示已归档** —— 默认关,只列 `archived=0` 行;开关开含归档行
- 活动行操作:**编辑** / **测试连接** / **删除**
- 归档行操作:仅 **恢复**(无编辑,无删除;归档是终态;恢复后需用户自己重填 api_key)
- 归档行视觉:整行删除线 + "已归档"徽标

#### 2. 新增/编辑 model (ModelDialog.vue)
- 字段: name / model (string, e.g. "gpt-4o-mini") / base_url / api_key / temperature / max_tokens / concurrency
- `archived` 不可在 dialog 里改 —— 只能通过"删除"或"恢复"
- 校验: name / model / base_url / api_key 必填;temperature / max_tokens / concurrency 数字
- **测试连接** 按钮:调 `test_model` 调一次 `provider.chat("ping")`,返回结构化 `TestModelReport`,**失败不抛错** —— 错误进 `report.error`,`latency_ms` 仍填(连 provider 创建失败都计超时)
- TestModelReport 字段完整展示:model / base_url / latency_ms / tokens_in / tokens_out / content_preview (前 200 字) / error

#### 3. 删除 model
- 弹 ConfirmDialog → 调 `delete_model(id)` → 后端 `UPDATE ... SET archived=1, api_key=""`
- **不**做级联删除;**不**删任何 batches / transformation_chapters 引用 —— 后续 worker 启动时仍能 resolve 到该行(尽管发请求会 401,error 冒到 mark_failed)
- 恢复:`restore_model(id)` → `UPDATE ... SET archived=0`;注意 `api_key` 仍为空,user 必须重新编辑保存

#### 4. 空 Models 表(首启或全归档后)
- 显式提示"尚未配置任何模型"。**不会**从 `NSC_DEFAULT_MODEL_*` 环境变量自动注入 —— 移除的 `seed_default_model_from_env` 静默兜底让"第一次跑应用就有 model" 看似贴心,实则排查链断在"为什么 worker 拿到的 model 不是我配的"上

### Design intent
- **`api_key` 一旦归档必抹**:用户原话"密钥不能随归档泄露"。即使行保留可恢复,密钥也不能保留
- **`ModelConfigRepo::get(id)` 不按 archived 过滤**:`BatchScheduler` 在 resolve `tc.model_config_id` 时必须能拿到归档行 —— 不然 `mark_failed` 路径找不到 `model.name` 写错误信息;`tc` 行的引用解析会断
- **`provider_factory` 重建是 cache miss 路径**:cache key 是 `model_config.id` 而不是 `(api_key, base_url)` —— 用户在 UI 改了 key 后,运行中的 worker 仍持有旧 provider 直到该 chapter 跑完;新建工作流才用新 key。显式 trade-off,避免运行中突然 401
- **`concurrency` 字段真正生效**:旧版只是 metadata,没人用;新版 worker 端用 `tokio::sync::Semaphore` 真的限流,`min(concurrency, 物理并发)` 是实际生效值(物理 worker 默认 2,模型管理可设上限 16)
- **空 Models 表显式提示**:**不**回退到"自动 seed 一个空壳 model 让 UI 不报错"那种 silent 兜底 —— 用户必须显式配置,空表就是空表

## Key data model

- `uploads` (migrations/0001,0007): sha256, filename, byte_size, file_path, original_text, word_count, uploaded_at
- `data_assets` (migrations/0004): upload_id (informational, 无 FK 无 UNIQUE), title, parsed_at, source_filename
- `chapters` (migrations/0005,0015): data_asset_id, idx, title, body TEXT (自包含正文), word_count
- `transformation_novels` (migrations/0006,0008): data_asset_id (fan-out), title, default_model_config_id, default_prompt_id, default_mode
- `batches` (migrations/0009,0012): transformation_novel_id, label, on_failure_policy, status (pending/running/stopped/paused/completed/terminated/cancelled), created_at/started_at/ended_at
- `transformation_chapters` (migrations/0010): 一行 = 一次转换尝试;batch_id, chapter_id, mode, prompt_id, model_config_id, ctx_prev_*, status (pending/running/done/failed/skipped/cancelled), result_content (新设计中常 NULL), tokens_in/out, error
- `workflow_results` (migrations/0011,0013): batch_id, created_at —— 一个 batch 一份 result set,持 chapter 级 content 真源
- `workflow_result_chapters` (migrations/0013): workflow_result_id, chapter_id, content (文本真源), created_at, updated_at
- `model_configs` (migrations/0001,0016): name, model, base_url, api_key, temperature, max_tokens, concurrency, archived (软删列), created_at/updated_at
- `prompts` (migrations/0001,0014,0017): name, kind (compress/style), template, is_builtin, archived (软删列), created_at/updated_at
- `ai_call_logs` (migrations/0018): 每次 LLM chat 落一行 —— business (transform_chapter/test_model), context_type/id (软引用), model_config_id (可空), model_name/base_url (反范式,断网可读), temperature/max_tokens, system_preview/user_preview/response_preview (各前 10KB), system_size/user_size/response_size (总字符数), estimated_tokens_in/actual_tokens_in/actual_tokens_out, status (success/failed), latency_ms, error, created_at
- `schema_versions`: 单 PK `version` 列,记录已应用的 migration 编号(Db::open 时跳过已应用项)
## Delete semantics

- **Delete upload**: preview_upload_deletion returns the list of derived data assets; the deletion is non-cascading. The UI shows the list and lets the user decide.
- **Delete data_asset**: cascades chapters + transformation_novels via FK.
- **Delete transformation_novel**: removes tn + its transformation_chapters only (不删 batches 行 —— 见 workflows 待重构章节)。
- **Delete chapter**: only allowed when no transformation references it (前端查 count_prompt_usage 等)。
- **Delete model** (软删): `UPDATE model_configs SET archived = 1, api_key = ""` —— 行保留供历史 tc 引用解析,**密钥必抹**(用户明确要求)。`restore_model` 取消软删,但 api_key 不会自动恢复(已被抹掉)。
- **Delete prompt** (软删): `UPDATE prompts SET archived = 1` —— builtin 行的 archive / restore / update 全部 fail-fast 拒绝。`restore_prompt` 取消软删,内容不校验。
- **Clear ai_call_logs**: 直接 `DELETE FROM ai_call_logs`(不软删),UI 显式按钮触发 ConfirmDialog。表的 status 列没有"软删"语义,只区分 success/failed。
- **Failure mode 统一**: 任何"用户删除 X 但 Y 引用 X"的场景,都要么 cascade 要么软删 + UI 提示,绝不静默丢数据。

## Upload module (post-refactor)

### Data
- uploads 表: sha256 / filename / byte_size / file_path / original_text / word_count / uploaded_at
- 一行 uploads 可派生 0~N 行 data_assets(upload_id 是 informational,无 FK 无 UNIQUE)
- 一行 data_asset 持有 N 行 chapters,每行 chapter.body TEXT 自包含正文

### UI / flow

#### 1. Library "上传" tab(列表)
- 列: filename / size / words / uploaded / **数据资产 N 个** / 操作
- "数据资产" 列来自 `daCountByUpload` computed:O(N) 扫 `store.dataAssets` 建 Map<upload_id, count>
- 操作按钮:**查看** / **解析章节** / **删除**
- "解析章节" 始终可点 —— 不再受 hasDataAsset 隐藏(新设计 upload/da 解耦)

#### 2. 上传文件(UploadDialog)
- 选 .txt → `upload_file` IPC → 写 uploads 行
- 后端一次算 word_count(中文感知:汉字 + 字母 + 数字)
- 不做内容清洗、不切章节;只是把整本原文存到 uploads.original_text

#### 3. Upload.vue `/library/upload/:id`(查看页)
- PageHeader actions:**保存** / **清洗** / **转为数据资产**
- 主区:textarea 编辑原文 + meta strip(大小/行数/字符数)
- onMounted 调 `get_upload` + `get_upload_text` 拉 metadata + 全文
- **保存**:
  - dirty 时可点(`rawText !== savedText`),调 `update_upload_text` 改 uploads.original_text
  - 不影响已有 data_asset —— chapters.body 自包含,改原文不会破坏已 commit 的 da
- **清洗**:
  - 不在 dirty 时可点,弹 CleaningDialog 预览 chars_delta / lines_delta
  - 确认后 `update_upload_text` 覆盖原文
  - **不再二次确认**(旧 byte_range 设计下需要"清洗会破坏章节范围"提示;新设计下提示已过时,已删)
  - 文件 > 10 MB 直接弹 alert,不让清洗
- **转为数据资产**:跳 `/library/upload/:id/parse` parse wizard

#### 4. parse.vue(章节解析,可多次进入)
- 进入: `watch(route.params.uploadId)` 触发 store.load —— **onMounted 不行,因为同组件复用不重新挂载**
- 拉 `get_upload_text` + `list_chapter_segments`(splitter 跑全文,后端忽略前端 markers/suppressed)
- 左侧章节列表:chaptersWithIdx(`map` 加 idx)+ `:key-field="'idx'` —— 原 `key-field="line"` 会让 SFC parser 误判默认值
- 右侧原文面板:RecycleScroller + `:key-field="'line'` —— 用 line 字段
- 用户操作:
  - **加 marker**(`addMarker(key)`):UI 高亮当前行,只用于 dirty 判断
  - **并入上一章**(`removeChapter(idx)`):把该段 content/word_count 追加到上一段 —— **真的合并**,不再仅 filter
  - **编辑章节 title**(`updateTitle`):存到 titleOverrides,key 是稳定的 content(不是 title)
- 提交: `commit_data_asset(uploadId, {title, chapters: [{title, content}]})` → 生成新 data_asset + chapters 行
- **同一 upload 可多次 commit** → 多个 da 共存(parse 后跳 `/library/data/:newDaId`)
- 离开 parse 页 `onUnmounted` 调 `store.unload()` —— 清空 rawText/source/workingChapters 等大对象,释放 pinia store 内存

#### 5. 删除 upload(在 Library "上传" tab)
- 按钮 → 调 `preview_upload_deletion` IPC 列出派生 da(`{id, title, chapters_count, tn_count}`)
- 弹 ConfirmDialog 警告:
  - 0 个派生 da: "Confirm delete upload \"X\"?"
  - >0 个派生 da:列出每个 da 的 id/title/chapters_count/tn_count,提示用户去 DataAssets tab 手动删
- 确认 → `delete_upload` 直接删 upload 行 + 文件
- **不 cascade**:删除 upload 后 data_assets / chapters 仍存在 —— "孤儿 da" 仍可访问(data_assets.source_filename 持久化,显示来源文件名)
- 数据资产页(`/data-assets`)的"已删除" badge 或类似标识:handoff 提示要显示,目前尚未实现

### Design intent

- **upload = 原始材料**(mutable original_text,可清洗)
- **data_asset = 加工产物**(chapters.body 自包含,独立于 upload)
- 同一小说可以"先上传 → 清洗一次 → commit 一份;再清洗 → 再 commit 第二份",生成多份独立 da
- chapters.body 不依赖 byte range:即使 upload 被删,da 的章节正文完整可用
- 派生关系(upload_id)是 informational:UI 用它显示来源 / 找孤儿,数据库不强制

### What's NOT done (TODO)

- "数据资产" tab 显示"已删除 upload"标记:da 的 `source_filename` 已持久化,但 UI 未做"upload 缺失" 警告
- Chapter title 编辑没限制 —— 用户可改成任何字符串,后续可能要做去重校验

## Transform flow

- BatchScheduler.create_workflow(spec) -> same-batch serial dispatch.
- JobQueue reads chapter.body and pushes to AiProvider.
- prev_original / next_original are taken from chapters.body directly (no upload.original_text lookup).

## Why the change

- The old byte-range model conflated bytes with chars in CJK text and made upload deletion implicit-cascade chapters of unrelated work.
- The new model keeps each chapter self-contained and decouples uploads from data assets.

## Open improvements (not done)

- TransformationNovelDetail could move into a Pinia store to remove ad-hoc refs.
- Status changes could be event-driven instead of 1s polling.
- chapters store markers / suppressed / titleOverrides use string keys today; could switch to chapter_id once IDs are exposed end-to-end.

## Test status

- `cargo test --workspace` 当前: nsc-core 18 unit tests + nsc-desktop 12 unit tests 全过,0 失败 0 警告
- 覆盖范围:
  - `recorder` 3 个(channel 满丢事件 / channel 收发 / noop)
  - `db::repo::ai_call_log` 6 个(insert+roundtrip / filter / clear / truncate_preview 边界)
  - `text::word_count` 6 个(中文 / 英文 / 混合 / 标点不计 / 空 / 前后空白)
  - `commands::cleaning` 5 个(规则 id 解析 / 合并折叠 / 错误路径)
  - `commands::transformation_novels` 8 个(snake_case DTO 反序列化 + 默认值边界)
  - `upload` 1 个(sha256 空字符串 known value)
  - `db::pool` 1 个(in-memory schema 跑通)
- byte-range 时代的测试已全部移除;migrations/0003_chapter_byte_ranges.sql + migrations/0015_chapter_body.sql 的迁移后,任何基于字节偏移的断言都不存在了
- `tests/` 目录下的 24 个 integration test 是 ignored placeholder,目前没实现(每个 crate 一个 `.rs` 占位);不在本次重构范围
- vue-tsc 4 个 pre-existing 错(`chapters.ts:176/265`,`Library.vue:264`,`parse.vue:201`),与本次重构无关,不在重构范围

## Prompts (提示词) refactor — completed

### Scope
- `crates/nsc-core/src/prompts/{mod,render,builtin}.rs` —— 渲染逻辑去 Result、system/user 分离、单次扫描替换
- `crates/nsc-core/src/models/{prompt,transformation}.rs` —— `TransformMode` enum 删除,统一用 `PromptKind`;Prompt 加 `archived` 字段
- `crates/nsc-core/src/db/repo/prompt.rs` —— 软删 (archive/restore)、`PromptUpdate` DTO 收紧 update 字段、builtin 行不可改
- `crates/nsc-core/src/db/repo/workflow_result.rs` —— 新增 `get_content_by_batch_and_chapter`,给 `prev_transformed` 拿真内容
- `crates/nsc-core/src/transformer/{transformer,queue,batch_scheduler}.rs` —— `prev_transformed` 真接 `workflow_result_chapters.content`;不再依赖已删除的 `tc.result_content`
- 新 migration `migrations/0017_prompt_archive.sql` —— 加 `archived` 列 + 索引
- 前端 `src/ipc/{types,commands}.ts` `src/stores/prompts.ts` `src/views/Prompts.vue` `src/components/PromptEditDialog.vue` —— 软删 UI、显示已归档开关、删除改用 ConfirmDialog
- `src-tauri/src/commands/prompts.rs` + `src-tauri/src/lib.rs` —— 新增 `list_prompts_including_archived` / `restore_prompt` / `count_prompt_usage` 命令

### Design intent

#### 1. enum 合并 (`TransformMode` → `PromptKind`)
- 历史原因: `transformation_chapters.mode` / `transformation_novels.default_mode` / `batches.overrides.mode` 三个字段语义完全一致 (compress/style),但代码里两个 enum (TransformMode / PromptKind) 各写一遍
- 改: 删 `TransformMode`,三处字段类型全用 `PromptKind`;serde 行为不变 (`"compress"` / `"style"`)
- 连带清理: `From<PromptKind> for TransformMode` impl 删;ts 类型 `TransformMode` 也删,内联 `'compress' | 'style'` 字面量

#### 2. prompt 渲染重构
- 旧: `render() -> Result<String>`、返回单串、内嵌 `--- system ---` 提示符让模型猜、7 次 `String::replace` 串行替换占位符
- 新:
  - `RenderedPrompt { system: Option<String>, user: String }` —— system / user 物理分离,后端构造 `Vec<ChatMessage>` 时直接拿
  - 模板里独占一行的 `---` 切分 system/user 段(典型 OpenAI 习惯,作者自己写时也无歧义)
  - 单次扫描 `fill_template` —— O(n) 一次,不再 7 次 replace
  - `join_chapter_pairs` / `join_transformations` 删 `header` 形参(只在前面加一行说明文本,直接拼)

#### 3. builtin prompt 强化
- 旧: `seed_default_model_from_env` 类似的沉默兜底 —— 模型缺失时静默注入默认,排查链被掐断
- 新: 不再 silent fallback;builtin 行是 user 可见的"出厂模板",可"复制"派生用户版再改;update builtin 行直接 `Err("内置 prompt 不可编辑 — 请先复制为用户 prompt")`(fail-fast)
- builtin 行的 `is_builtin` 字段不可被 `PromptUpdate` 改,杜绝误操作

#### 4. soft delete + `PromptUpdate` 收紧
- `migrations/0017_prompt_archive.sql` 加 `archived INTEGER NOT NULL DEFAULT 0` + 索引
- `PromptUpdate<'a>` 只接受 `id` / `name` / `kind` / `template` 四个字段 —— 从 DTO 层杜绝 is_builtin / archived 误改
- `list_prompts()` 默认隐藏归档;新增 `list_prompts_including_archived()` 给 UI 切"显示已归档"开关
- `delete_prompt(id)` 改 `archive_prompt(id)` 语义(后端实现 `UPDATE ... SET archived=1`)
- `restore_prompt(id)` 解除归档
- `transformation_chapters.prompt_id` 历史引用 —— 行保留可反查 prompt name / kind / template;UI 列已归档行加徽标 + 删除线 + 恢复按钮

#### 5. `prev_transformed` 修复
- 旧: `prev_transformed` 收集用 `tc.result_content`,但 `tc` 表 (`transformation_chapters`) 不存历史结果内容,实际拿不到,前端显示空白
- 新: `workflow_result_chapters.content` 是真内容源;`repo::workflow_result::get_content_by_batch_and_chapter(batch_id, chapter_id) -> Result<Option<String>>`
- 队列 `Prep.prev_tx: Vec<(String, String)>` —— (chapter_title, content) 列表,带 title 给模型对齐参考

#### 6. `PromptEditDialog` 校验强化
- `canSubmit` 多一条: template 必须含 `{{chapter_content}}` —— 按钮直接禁用,避免漏提交没引用章节正文的 prompt
- 占位符常量 `CHAPTER_CONTENT_PLACEHOLDER` 提到 script 顶部,模板与提示共用同一份
- `missingChapterContent` warning 仍保留(给用户看"为什么不能保存"),但硬校验已在前

#### 7. UI 一致性
- "显示已归档" 开关、archived 徽标 + 删除线、恢复按钮 —— 与 Models.vue 同模式
- 删除确认改用 `ConfirmDialog` 组件 —— 替代原内嵌 `Dialog` + 自定义 footer
- `kind` 列用 `Tag` 组件,替代裸 `<span class="kind-tag">` —— 与 Models.vue 的 kind 列保持一致
- `countPromptUsage` IPC 名改 `count_prompt_usage`(短名),与命令含义对齐

### What's NOT done (TODO)

- builtin prompt 列表当前是 hard-coded 在 `prompts/builtin.rs`;未来若需"用户提交 builtin"可走标准 insert + is_builtin=1 通道,目前未实现
- 恢复 prompt 时不做内容校验 —— 万一 builtin 模板已升级,恢复后会与新 builtin 不一致,需要用户自己处理


## AI call logs (AI 调用日志) refactor — completed

### Scope
- 新表 `migrations/0018_ai_call_logs.sql` —— 每次 LLM chat 调用落一行(成功 / 失败都记)
- 后端:`models/ai_call_log.rs` + `db/repo/ai_call_log.rs` + `commands/ai_call_logs.rs` + 4 个 IPC 命令
- 新模块 `recorder/mod.rs` —— `AiCallRecorder` trait + `NoopRecorder` + `ChannelRecorder` + 后台 writer,Phase 2 接入用
- 前端:`src/views/AiCalls.vue` + `src/components/AiCallDetail.vue` + sidebar 一项 + router `/ai-calls`
- 9 个新测试(6 repo + 3 recorder)

### Design intent

#### 1. preview 截断 + size 记录
- prompt / response **不存全文** —— 一章 50KB+,1000 章就让 DB 爆炸
- 只存前 10KB 预览 + 总字符数(`system_size` / `user_size` / `response_size`)
- 完整内容由调用方自己保留:transform 路径在 `transformation_chapters.result_content`;test_model 路径内容由用户在 UI 看完即丢
- `truncate_preview()` 是公开函数,在 repo 边界统一截断,recorder / 命令层都复用 —— 杜绝"哪边少截 1 字符"的隐性不一致

#### 2. estimated_tokens_in 启发式
- 用 `chars / 2` 粗估(zh-aware 经验值)
- 引入 tiktoken / tokenizer 太重,启发式够排查用 —— UI 标注"粗估",让用户理解非精确
- 实际值来自 provider `usage.prompt_tokens` / `completion_tokens`,缺 usage 时为 NULL

#### 3. 业务归类
- `business` enum 当前两个值:`transform_chapter` / `test_model`
- `context_type` / `context_id` 软引用(无 FK)—— `transformation_chapter` 删了不影响日志可见
- 未来加 `embedding` / `summarize` / `*_eval` 扩展 enum 即可,不破坏现有 schema

#### 4. denormalized model 信息
- `model_name` / `base_url` 直接存,不复用 `model_configs.name` ——
  model_config 可能被 archive / 删,日志行要能看到"当时调的是哪个端点哪个 model"
- `model_config_id` 可空 —— 历史日志 + 孤儿配置的两端兜底

#### 5. recorder 抽象(Phase 2 接入用,Phase 1 已就位)
- `AiCallRecorder` trait:`record(event)` 不阻塞,`pending()` 返回队列深度
- `NoopRecorder`:单元测试 / `Db::open_in_memory` 场景
- `ChannelRecorder`:mpsc channel + AtomicU64 计数,channel 满时 `try_send` 失败 → drop new
- `spawn_writer` / `run_writer`:后台 task 按 `db_path` 重开 DB,逐条 `ai_call_logs.insert`
- DB 在 hot path 不出现,跨线程 / Send/Sync 摩擦归零

#### 6. UI 一致性
- "AI 调用" 作为顶级 sidebar 项(`/ai-calls`),与"模型"同层
- 表格列:时间 / 业务 / 模型 / 状态 / tokens(估→实 in / 实 out)/ 延迟 / 错误
- 过滤:业务 / 状态 / model_config_id 三个下拉 + 输入;任一改触发 reload
- 详情 Dialog:`AiCallDetail.vue` —— 7 段(基本信息 / 模型配置 / Tokens / system / user / response / 错误)分块展示
- 清空用 `ConfirmDialog`(与 Models / Prompts 同模式),不弹浏览器原生 confirm
- 失败行用红色 Tag + 错误列 tooltip 完整文本

### What is NOT done

- 自动清旧(超过 N 天):用户没要,留按钮手动 clear
- 全文检索(在 system / user_preview 上 LIKE):不做 —— preview 是限长片段,搜不到是预期
- 体积监控 / 看板:每次 list 看 `共 N / 限 limit` 即可,没做"快满了"提醒
- detail 页"查看完整 prompt/response 链接":transform 路径要跳 `transformation_chapters.result_content` 详情(未来再做)

## AI call logs — Phase 2 (recorder 接入)

### Scope
- `transformer/transformer.rs` —— `DefaultTransformer` 加 `recorder` 字段,`transform()` 内部包 `Instant` + 拼装 `AiCallEvent` + `recorder.record(event)`
- `transformer/queue.rs` —— `JobQueue::new` 加 `recorder: Arc<dyn AiCallRecorder>` 参数,clone 给每个 worker,worker 传给 `DefaultTransformer::new`
- `transformer/transformer.rs` —— `TransformRequest` 加 `transformation_id: i64` 字段(recorder 记 context_id 用)
- `src-tauri/lib.rs` —— 启动建 `ChannelRecorder(4096)` + `spawn_writer(path, recorder, rx)` + `.manage(recorder)` 作为 Tauri state + 传 JobQueue 第四参数
- `src-tauri/commands/models.rs` —— `test_model` 加 `recorder: State<<'_, Arc<dyn AiCallRecorder>>` 参数 + 拼 event + record
- 0 新测试(transformer 集成测试需要 mock provider + mock recorder,价值不大;recorder / repo 单测已覆盖)

### Design intent

#### 1. recorder 接入位置
- **transformer 路径**:`DefaultTransformer::transform` 包住 `ai.chat()` 调用,instrumentation 与业务同处一个函数,延迟/响应/错误都能精确记录。prep 失败路径(上下文准备出错)不 record —— 那时根本没发起 chat,记录没意义。
- **test_model 路径**:commands 层直接 wrap。test_model 不走 JobQueue,只调一次 provider.chat,自带 `Instant`。两条路径互不耦合。
- **prep 失败路径不 record**:跑不到 AI 调用就报错(DB / 文件),记了反而误导。这是显式选择。

#### 2. latency 测量
- 包在 `ai.chat()` 调用周围,含网络往返 + provider 内部序列化 + tokio 调度。
- **不**算上 prep / db write —— 那不在 AI 调用边界。
- test_model 也类似:包住 OpenAiProvider::new + chat(),含 provider 构造开销。

#### 3. estimated_tokens_in
- 在 transformer: `(system.chars + user.chars) / 2`(zh-aware 粗估)
- 在 test_model: `user.chars / 2`(test_model 只发 "ping",无 system)
- UI 列名"粗估",与实际值并列展示,让用户自己判断偏差

#### 4. 失败路径也 record
- success / failed 都 record —— 看板才能算"成功率 / 平均延迟"
- failed 行带 `error` 字段(完整 provider error 字符串),UI 用红色 Tag + tooltip
- **不**因 record 失败影响业务:recorder.record 不阻塞,channel 满 → drop new

#### 5. 启动 lifecycle
- `ChannelRecorder::new(4096)` 创建 channel + recorder,后台 writer 任务从通道拿 event 落库
- channel 容量 4096 远超 worker 并发上限(默认 2 worker + 1 test_model),饱和概率极低
- `spawn_writer` 内部 `std::thread::spawn` + 内建 `tokio::runtime::Builder::new_current_thread()`,跟 `JobQueue` worker 同一种解耦风格 —— 不依赖调用方线程是否有 tokio reactor(这点很重要:`src-tauri/lib.rs::run()` 是 builder 同步阶段,直接 `tokio::spawn` 会 panic "there is no reactor running")
- `std::thread::JoinHandle` 留着不用:app 退出 → sender 被 drop → channel 关闭 → writer recv 返回 None → loop break → runtime drop → thread 自然退出,最后几行日志能落完
- **不**主动 abort,避免截断最后几行

#### 6. context_id 语义
- transformer 路径:`context_type=transformation_chapter` + `context_id=tid`
- `transformation_chapters` 行可能后续被删,但日志行不受影响 —— 当前未提供"按 tc id 反查 AI 调用"接口,如需可后续单独加
- test_model 路径:无 context_type / context_id(单次连通性测试,无业务对象)
### What is NOT done

- **transformer 集成单测**:需要 mock `AiProvider` + mock `AiCallRecorder`(`mockall` / 手写 test double),投入产出比低。当前依赖 recorder 单测 + repo 单测 + 手动 dev 验证。
- **detail 页"看完整 prompt / response"链接**:transform 路径的全文在 `transformation_chapters.result_content`(已 commit 的章节),目前详情页只展示 preview。跳转逻辑后续单独做。
- **自动清旧 / 体积监控**:用户没要,手动 clear 按钮已够。


## AI call logs — 收尾

### Scope
- 删 `list_ai_call_logs_by_context` 整条 dead chain(后端 IPC + repo 方法 + lib.rs invoke_handler 注册 + 前端 wrapper):UI 没有任何调用入口,留着只会让"以为能用"的后来者 debug 半天才发现没接上
- 删 `transformer/transformer.rs` 末尾的 `let _ = req.prompt.kind; let _ = PromptKind::Compress;` —— 上一轮 AI 为消 `unused` warning 留下的遮掩代码,跟 fail-fast 偏好冲突
- 顺带从 `transformer.rs` 的 `use crate::models::{...}` 列表里去掉 `PromptKind`(同上,不再需要)

### Design intent

#### 1. dead chain 判定
- "无引用"不是单点证据,要沿着 IPC 串正向走一遍:
  - `src-tauri/src/lib.rs::generate_handler!` 里有 → 命令层有 → repo 有 → 前端 ipc/commands.ts 里有 → 前端代码里 `import` 后真有用
- 上面任一环节断掉,就视为 dead,一刀删干净
- 不要为了"未来可能要用"保留:真有需求时再写,也不费事 —— 而且下次写的时候背景可能完全变了,留着旧的半成品反而误导

#### 2. `let _ = ...` 遮掩代码判定
- 任何 `let _ = some_field;` + 配套注释("避免 unused 警告")出现时,先问:**这个字段真的没用吗?**
- 如果真的没用:删 import + 字段传递链路 + 字段本身,让编译器告诉你哪里还在用(而不是 `let _` 假装在用)
- 如果只是当前路径没用但别的路径要用:**让字段在当前路径上派真正的用场** —— 比如 transformer 里把 `prompt.kind` 透传到 recorder event,而不是 `let _ = req.prompt.kind` 后丢掉
- 本次 `prompt.kind` 的情况属于"render 路径未来要用"(`PromptContext.kind` 在 render.rs 是占位),所以不强行做;只删 `let _`,让 transform 内部不再有 PromptKind 的 import,代码读起来清晰

#### 3. 为什么不对 `recorder::eprintln!` 也"清理"
- recorder 失败时 `eprintln!("[recorder] channel full, dropping event")` / `insert failed: ...` 直接打到 stderr
- 想要消掉得引入 `tracing` / `log` crate,但项目当前没引入日志框架,引入属于大改造
- 这次只删"明确的死代码",不动需要架构支撑的清理项 —— 后者单独立 todo

### What's NOT done(本次范围之外)
- **trace 化 recorder 失败日志**:需要引入 tracing,见上节
- **transformer 集成单测**:已有 todo,本次未触及
- **detail 页"看完整 prompt/response"跳转**:已有 todo,本次未触及



## AI call logs — 启动 panic 修复

### Scope
- `crates/nsc-core/src/recorder/mod.rs::spawn_writer` 改写:不再 `tokio::spawn(...)`,改为 `std::thread::spawn` + 内置 `tokio::runtime::Builder::new_current_thread()`
- 返回类型 `tokio::task::JoinHandle<()>` → `std::thread::JoinHandle<()>`,调用方 `let _writer_handle = ...` 不变
- `src-tauri/src/lib.rs` 与本文件 "AI call logs — Phase 2" 的 "启动 lifecycle" 段同步更新

### Design intent

#### 1. 根因
- `src-tauri/lib.rs::run()` 是 builder 同步阶段,在 `tauri::Builder::default().run(...)` 之前
- `recorder::spawn_writer(...)` 内部 `tokio::spawn(...)` —— 此时**还没有 Tauri 的 tokio reactor**
- 直接 panic:`thread 'main' panicked at crates\nsc-core\src\recorder\mod.rs:126:5: there is no reactor running, must be called from the context of a Tokio 1.x runtime`
- exit code 101,应用启动即挂

#### 2. 修法 —— 跟 JobQueue worker 同一种解耦风格
- `JobQueue` 的 worker 早就处理过类似问题:`std::thread::spawn` + 内置 `tokio::runtime::Builder::new_current_thread()`,完全不依赖调用方线程上下文
- `spawn_writer` 直接套用:内部建一个 current_thread runtime,writer 跑在里面
- 调用方线程是否有 reactor / 是否在 tokio runtime 里,都不影响 recorder 工作 —— recorder 自给自足
- `src-tauri/lib.rs` 的 `let _writer_handle = ...` 保持不变;`let _` 模式丢弃 handle 的语义也保持(app 退出时 thread 自然结束)

#### 3. 启示 —— 后台任务尽量别假设调用方有 tokio runtime
- 任何模块对外提供的 `spawn_*` / `start_*` helper,如果用 `tokio::spawn` 直接起 task,**应该默认自带 runtime**,不要让调用方保证
- 这个 panic 在第一次跑应用时立刻暴露;如果只在测试里 mock 就永远发现不了(测试 in-memory + 直接 `rt.block_on` 不会触发)
- 同类问题可能潜伏在 `BatchScheduler` / `startup_recovery` 等其他 module,review 时关注



## Workflows (转换 / batch) — 待重构

**状态**: 计划已拍板,0 代码改动。**实际页面操作未跑通**(用户自述) —— 之前 panic + 启动问题修完后才能手动测。

### 当前结构
- `src-tauri/src/commands/workflows.rs` (12.5KB): 8 个 IPC 命令 + 4 处内联 SQL + 2 个手写 status parser + 4 个聚合结构体 + 1 个 CONTENT_PREVIEW_CHARS 常量
- `crates/nsc-core/src/models/{batch,transformation}.rs`: 已有 `BatchStatus` / `TransformStatus` enum + 私有 `str_to_status` / `str_to_policy` helper(repo 内部用,不能改)
- `crates/nsc-core/src/db/repo/{batch,workflow_result,novel}.rs`: 现有 `BatchRepo::get/list_by_tn/set_status` 等,但 4 处跨表 join 还在 commands/workflows.rs 里

### Scope(下一步要做的)
1. **`BatchStatus` / `TransformStatus` 加 `TryFrom<&str>`** —— 取代 commands/workflows.rs 里手写的 `parse_batch_status` / `parse_transform_status`(7+6 行 match)。serde `rename_all = "snake_case"` 已配置,可直接对接
2. **4 个 IPC 聚合类型搬到 `models/workflow.rs`(新文件)**:`WorkflowSummary` / `WorkflowChapterRow` / `SourceChapterRow` / `ChapterWorkflowResultRow`。字段名是 IPC 契约,前端已经在用,保持完全兼容
3. **4 处内联 SQL 搬到 typed repos**:
   - `BatchRepo::summary_counts(batch_id)` → 取代 `to_summary` count 查询(transformation_chapters 聚合)
   - `BatchRepo::list_chapters_with_results(batch_id)` → 取代 `list_workflow_chapters`(transformation_chapters + chapters + workflow_result_chapters 三表 join)
   - `WorkflowResultRepo::list_for_chapter(tn_id, chapter_id)` → 取代 `list_chapter_workflow_results`(batches + workflow_results + workflow_result_chapters + transformation_chapters 四表 join)
   - `TransformationNovelRepo::list_source_chapters(tn_id)` → 取代 `list_transformation_source_chapters`(chapters + 子查询 workflow_result_chapters count)
4. **小 helper**:`fn lock_db(state: State<'_, Arc<Mutex<Db>>>) -> Result<Db, String>` + 在 BatchRepo 加 `get_required(id) -> Result<Batch, String>`(不存在 → `"batch X 不存在"`)
5. **保留 IPC 签名稳定** —— 前端不动

### 不动 / 保留的
- `BatchScheduler` (`crates/nsc-core/src/transformer/batch_scheduler.rs`) 内的内联 SQL —— 那是事务性 multi-table atomic ops(创建 batch + 创建空 result set + 批量插 tc 行),repo 抽象不好套;留给后续单独评估
- 前端 `stores/workflows.ts` / `views/TransformationNovelDetail.vue` 等 —— IPC 契约不变
- `batches.on_failure_policy` 字段、3 种 policy 语义 —— 跟转换流程强耦合,workflows 重构范围之外

### 待验证(用户那边测)
- `create_workflow` 入口:`tn_id` / `chapter_ids[]` / `prompt_id` / `model_config_id` / `mode` / 三个 ctx 窗口数 —— 现有表单是否能完整提交
- `stop_workflow` / `retry_workflow_chapters`:scheduler 接口 + repo 重写后,semantics 是否一致
- `list_chapter_workflow_results`:4 表 join 搬到 `WorkflowResultRepo::list_for_chapter` 后,行序/字段是否一致
- 失败路径:prompt_id 找不到 / model_config_id 已软删 / chapter_id 不在 tn 的 data_asset 范围 —— 各报什么错

### Design intent(沿用全局原则)
- **不**为求 repo 一致性而把 `BatchScheduler` 的事务 SQL 也拆出去 —— 那是 atomicity 边界,拆了反而引入新的并发问题
- **不**为减少一个文件而把 4 个聚合类型塞进 `models/batch.rs` —— 它们跨 batch / transformation_chapters / workflow_result_chapters 三表,独立文件 `workflow.rs` 更清晰
- IPC 字段名一字不改:前端 `WorkflowSummary` / `WorkflowChapterRow` 等已经引用 `tc_id` / `chapter_id` / `chapter_idx` 等 snake_case,改名 = 静默破坏
- `TryFrom<&str>` 不引入 `serde_plain` 等额外 crate,手写 match 跟现有 `str_to_status` 几乎一致,避免依赖爆炸

### Open questions(等用户测时反馈)
- `WorkflowChapterRow.content_preview`: 来自 `wrc.content` 前 80 chars。如果某章特别长,要不要分页 / 折叠?
- `list_workflow_chapters` 当前没分页 —— batch 内 chapter 数 = 全本 chapter 数(可能上千),要不要加 limit / offset?
- `list_chapter_workflow_results` 返回历史所有 batch 对该 chapter 的结果,要不要按 status 过滤(只 done / 含 failed)?
