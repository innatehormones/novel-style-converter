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
