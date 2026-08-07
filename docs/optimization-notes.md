# novel-style-converter Business Description (post-refactor)

## Two independent data blocks

1. Upload: raw file with sha + filename + size + original_text (editable for cleaning).
2. DataAsset + Chapter: a parsed package. Chapters carry the actual text inline via chapters.body (no more byte-range slicing of the upload).

A single upload can produce many data assets, and the data assets survive the upload being deleted. The link data_assets.upload_id is informational only (no FK, no UNIQUE).

## Optimization log

### Upload refactor — completed

- Uploads now represent only the original file and its editable source text; uploading does not implicitly create or replace a data asset.
- Chapter parsing can be committed repeatedly from the same upload, producing independent data assets.
- Committed chapters store their complete text in `chapters.body`; later upload cleaning, editing, or deletion does not invalidate existing data assets.
- Upload deletion is non-cascading. Before deletion, the UI previews derived data assets and only prompts the user to remove those assets manually from the data asset module.
- Data asset counts are visible in the upload list, and the parse entry remains available even when the upload already has derived data assets.
- The obsolete warning that cleaning would destroy existing chapter ranges was removed because chapter content is now self-contained.
- Parse-page state is isolated from the upload source after commit; leaving the page unloads large temporary text and chapter collections.

This section records the currently agreed upload boundary and is the baseline for reviewing the next module.

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

## Key data model

- uploads: sha256, filename, byte_size, file_path, original_text, word_count
- data_assets: upload_id (informational), title, parsed_at, source_filename
- chapters: data_asset_id, idx, title, body TEXT, word_count
- transformation_novels: data_asset_id (fan-out)
- batches / transformation_chapters / workflow_results: scheduler + result set

## Delete semantics

- Delete upload: preview_upload_deletion returns the list of derived data assets; the deletion is non-cascading. The UI shows the list and lets the user decide.
- Delete data_asset: cascades chapters + transformation_novels via FK.
- Delete transformation_novel: removes tn + its transformation_chapters only.
- Delete chapter: only allowed when no transformation references it.

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

cargo test -p nsc-core runs an ignored placeholder per file. Old tests referenced byte-coordinate assertions and now-deprecated API shapes. They are flagged for rewrite against migrations/0015_chapter_body.sql and the new repo methods.

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

- **Phase 2**: `DefaultTransformer::transform` + `commands::models::test_model` 没接 recorder ——
  recorder 接口 + ChannelRecorder + spawn_writer 已就绪,但还没在 hot path 调 `record(event)`
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
- `JoinHandle` 留着不用:app 退出 → tokio runtime 自然结束 → channel drop → writer recv 返回 None → loop break,最后几行日志能落完
- **不**主动 `abort()`,避免截断最后几行

#### 6. context_id 语义
- transformer 路径:`context_type=transformation_chapter` + `context_id=tid`
- `transformation_chapters` 行可能后续被删,但日志行不受影响 —— 反查"哪个 tc 行调过 AI"仍能查到(用 `list_ai_call_logs_by_context`)
- test_model 路径:无 context_type / context_id(单次连通性测试,无业务对象)

### What is NOT done

- **transformer 集成单测**:需要 mock `AiProvider` + mock `AiCallRecorder`(`mockall` / 手写 test double),投入产出比低。当前依赖 recorder 单测 + repo 单测 + 手动 dev 验证。
- **detail 页"看完整 prompt / response"链接**:transform 路径的全文在 `transformation_chapters.result_content`(已 commit 的章节),目前详情页只展示 preview。跳转逻辑后续单独做。
- **自动清旧 / 体积监控**:用户没要,手动 clear 按钮已够。
