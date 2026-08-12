// 字段命名约定:
// - 响应类型(后端 `#[derive(Serialize)]` 的实体,如 `ModelConfig` / `Upload`)
//   保持 snake_case,与 `crates/nsc-core/src/models/*.rs` 一一对应。
// - IPC 入参的 *命令参数名*(`payload` / `id` 等)由 Tauri 自动 camelCase 化,
//   所以外层 `invoke('cmd', { ... })` 的 key 用 camelCase。
// - IPC 入参的内层 DTO(后端显式 `#[serde(rename_all="snake_case")]` 的)
//   必须按 snake_case 原样发,前端 wrapper 不要做任何 inline 改名。
// 字段变更必须同步修改后端 DTO + 本文件 + commands.ts 中对应 wrapper。

/**
 * 后端 `model_configs` 行的前端镜像。
 * - `api_key` 明文存 SQLite,前端拿到也要原样回传(不要脱敏 —— 提交时仍需要真实值)
 * - `concurrency` 是 per-model 并发上限:worker 端按 `model_config_id` 共享信号量。
 *   有效范围 [1,16];超过物理 worker 数(默认 2)不会触发额外阻塞,但下限 1 防止 0 死锁。
 * - `archived = 1` 表示软删(API key 已被清空);仍保留在 list 响应里供 UI 展示历史 model。
 * - 字段全部 snake_case 来自后端 serde(后端**不**做 rename)
 */
export interface ModelConfig {
  id: number;
  name: string;
  base_url: string;
  api_key: string;
  model: string;
  max_tokens: number | null;
  temperature: number | null;
  concurrency: number;
  archived: number;
}

/**
 * `upsert_model` / `test_model` 入参:`id === 0` 表示新建,否则按 id 更新。
 * 这是后端 snake_case DTO(内层字段原样发,不要 inline 改名)。
 */
export type ModelConfigInput = Omit<ModelConfig, 'id' | 'archived'> & { id: number };

/**
 * `test_model` 结构化返回：
 * - 成功：`content_preview` 填响应前 200 字符，`tokens_in/out` 来自 provider usage。
 * - 失败：`error` 填完整字符串（provider 创建失败 / 非 2xx / 空 choices / 缺 usage 都会写）；
 *   `content_preview` 与 tokens 全为 null。
 * - 任意路径都会填 `latency_ms`（创建 provider 失败也计超时）。
 */
export interface TestModelReport {
  model: string;
  base_url: string;
  latency_ms: number;
  tokens_in: number | null;
  tokens_out: number | null;
  content_preview: string | null;
  error: string | null;
}

/// State 1: 原始上传文件元数据。不含章节结构(章节在 data_assets)。
export interface UploadSummary {
  id: number;
  sha256: string;
  filename: string;
  byte_size: number;
  uploaded_at: string;
  file_path: string;
  /// zh-aware 字数(汉字 + 字母 + 数字),upload_file 时后端一次算好。
  word_count: number;
}

/// 数据资产类型 — source 是原始解析产物,promoted 是从工作流结果派生的新资产。
export type DataAssetKind = 'source' | 'promoted';

/// 单条 data_asset 元数据(供 promote_workflow / list_data_assets_by_upload 等返回)。
export interface DataAsset {
  id: number;
  upload_id: number;
  title: string;
  parsed_at: string;
  source_filename: string;
  kind: DataAssetKind;
  source_workflow_id: number | null;
  source_data_asset_id: number | null;
  note: string;
}

/// State 2: 一次解析结果 = 一份 data_asset + 一组分章节切片。
export interface DataAssetSummary {
  id: number;
  upload_id: number;
  title: string;
  parsed_at: string;
  source_filename: string;
  tn_count: number;
}

/// Library.vue "数据资产" tab 行:data_asset 元数据 + 来源 upload 文件名 + 章节总字数。
export interface DataAssetRow {
  id: number;
  upload_id: number;
  title: string;
  parsed_at: string;
  filename: string;
  byte_size: number;
  /// SUM(chapters.word_count) WHERE data_asset_id = da.id。
  word_count: number;
  /// COUNT(transformation_novels.id) WHERE data_asset_id = da.id。
  tn_count: number;
  /// 资产类型:source = 原始解析;promoted = 从工作流结果派生。
  kind: DataAssetKind;
  /// 当 kind=promoted 时,记录源 workflow(batch.id);source 时为 null。
  source_workflow_id: number | null;
  /// 当 kind=promoted 时,记录源 data_asset.id;source 时为 null。
  source_data_asset_id: number | null;
  /// 用户备注。
  note: string;
  /// 派生出多少 promoted da(仅 source 类型有值;promoted 类型始终 0)。
  promoted_count: number;
}

/// State 2 章节元数据(从 list_data_asset_chapters 返回)。正文由前端按 byte 切片 original_text。
export interface DataAssetChapter {
  id: number;
  idx: number;
  title: string;
  body: string;
  word_count: number;
  /// 章节来源:transformed = 工作流转换结果;original = 原文(派生 da 失败章节回退)。
  source_kind: 'transformed' | 'original';
}

/// commit_data_asset 入参:title + 章节列表(每个含 title + byte 范围)。
export interface CommitDataAssetInput {
  title: string;
  chapters: Array<{
    title: string;
    content: string;
  }>;
}

export interface ChapterSegment {
  title: string;
  content: string;
  word_count: number;
}

export interface ChapterMeta {
  id: number;
  idx: number;
  title: string;
  word_count: number;
}

/**
 * `get_chapter_contents` 返回:章节正文预览(预览页用)。内容是后端从
 * `uploads.original_text` 按 byte range 切片后,剥首行标题再 trim。
 */
export interface ChapterContentRow {
  idx: number;
  title: string;
  content: string;
}

/// 章节切片实体。byte_start/byte_end 永远在 upload.original_text 坐标系。
export interface Chapter {
  id: number;
  data_asset_id: number;
  idx: number;
  title: string;
  body: string;
  word_count: number;
  /// 章节来源:transformed = 工作流转换结果;original = 原文(派生 da 的失败章节回退)。
  source_kind: 'transformed' | 'original';
  /// 派生时指向源 chapter.id(只在派生 da 里有值)。
  source_chapter_id: number | null;
}

/**
 * `commit_data_asset` / `parse_chapters` 入参的章节元素:
 * 仅标题 + byte 范围。后端按 byte range 切片原文计算 `word_count` / `idx`。
 */
export type ChapterInput = {
  title: string;
  content: string;
};

/**
 * `list_transformation_novels` 返回:转换小说元数据。
 * `chapters_count` 是该 `data_asset_id` 下所有 chapters 的总数,
 * 不代表这本 tn 实际有多少 transformation_chapter 行。
 */
export interface TransformationNovelSummary {
  id: number;
  data_asset_id: number;
  title: string;
  created_at: string;
  chapters_count: number;
  default_model_config_id: number | null;
  default_prompt_id: number | null;
  default_mode: 'compress' | 'style' | null;
}

/**
 * `create_transformation_novel` 入参:后端 snake_case DTO,
 * 三个默认字段为可空,内层字段原样发,不要 inline 改名。
 * 命名加 Input 后缀,与后端 `*Payload` 区分,避免跨语言同名歧义。
 */
export interface CreateTransformationNovelInput {
  data_asset_id: number;
  title: string;
  default_model_config_id?: number | null;
  default_prompt_id?: number | null;
  default_mode?: 'compress' | 'style' | null;
}

/**
 * `update_transformation_novel` 入参:后端 snake_case DTO,三个默认字段可空。
 * null 表示清空存量默认值(后端 update 行为:用 payload 覆盖 cur.default_*)。
 */
export interface UpdateTransformationNovelInput {
  id: number;
  title: string;
  default_model_config_id?: number | null;
  default_prompt_id?: number | null;
  default_mode?: 'compress' | 'style' | null;
}

// === Workflow 工作流 ===
/** 后端 `BatchStatus` 收敛到两态:`running` / `stopped`(spec §3.3)。Stopped 后只能 retry 空槽。 */
export type WorkflowStatus = 'running' | 'stopped';

/**
 * `list_workflows` / `get_workflow` 返回:工作流汇总 + 章节计数。
 * counts 直接嵌在行内 —— 不用单独调 count 接口。
 */
export interface PromoteWorkflowInput {
  batchId: number;
  title: string;
}

export interface WorkflowSummary {
  id: number;
  tn_id: number;
  label: string | null;
  status: WorkflowStatus;
  created_at: string;
  started_at: string | null;
  ended_at: string | null;
  done_count: number;
  failed_count: number;
  skipped_count: number;
  total_count: number;
  promoted_count: number;
}

/** `list_workflow_chapters` 返回:tc 行 + 章节标题/idx + 关联结果槽预览。 */
export interface WorkflowChapterRow {
  tc_id: number;
  chapter_id: number;
  chapter_idx: number;
  chapter_title: string;
  status: 'pending' | 'running' | 'done' | 'failed' | 'skipped';
  error: string | null;
  content_preview: string | null;
  is_empty_slot: boolean;
}

/** `list_transformation_source_chapters` 返回:tn 下全部源章节 + 非空结果数。 */
export interface SourceChapterRow {
  chapter_id: number;
  idx: number;
  title: string;
  word_count: number;
  non_empty_result_count: number;
}

/** `list_chapter_workflow_results` 返回:某源章节在所有工作流里的结果(按 batch_id DESC)。 */
export interface ChapterWorkflowResultRow {
  batch_id: number;
  batch_label: string | null;
  batch_status: WorkflowStatus;
  batch_ended_at: string | null;
  content: string | null;
  status: 'pending' | 'running' | 'done' | 'failed' | 'skipped';
}

/** `create_workflow` 入参:后端 snake_case DTO,所有字段必填(spec §5.1)。 */
/**
 * `create_workflow` 入参:后端 snake_case DTO,所有字段必填(spec §5.1)。
 *
 * `on_failure_policy` 是章节失败时的处理策略:
 * - `pause_and_review`: 失败时 batch 转 Paused,等用户在 modal 里手动决策(重试/跳过/终止)
 * - `terminate`:        失败时同 batch 后续章节 cancelled + batch 转 Terminated
 * - `skip_failed`:      失败时该章标 Skipped,继续派下一章(batch 留 Running)
 */
export interface CreateWorkflowInput {
  tn_id: number;
  label: string | null;
  chapter_ids: number[];
  prompt_id: number;
  model_config_id: number;
  mode: 'compress' | 'style';
  ctx_prev_original: number;
  ctx_prev_transformed: number;
  ctx_next_original: number;
  on_failure_policy: 'pause_and_review' | 'terminate' | 'skip_failed';
}

/**
 * `transformation_chapters.status` 状态机:
 * `pending` → `running` → (`done` | `failed` | `cancelled`)
 * 失败不自动重试 — 用户手动调 `enqueue_transformation_chapters` 重排队。
 */
export type TransformStatus = 'pending' | 'running' | 'done' | 'failed' | 'skipped' | 'cancelled';

/**
 * `list_transformation_chapters` / `list_transformation_chapters_for_chapter` 返回:
 * 一次转换任务的完整状态。`chapter_idx` / `chapter_title` 是 join `chapters` 表拼上的,
 * 方便 Transform 页直接展示,无需二次请求。
 */
export interface TransformationChapterRow {
  id: number;
  transformation_novel_id: number;
  chapter_id: number;
  chapter_idx: number;
  chapter_title: string;
  mode: 'compress' | 'style';
  prompt_id: number;
  model_config_id: number;
  status: TransformStatus;
  result_content: string | null;
  tokens_in: number | null;
  tokens_out: number | null;
  error: string | null;
  started_at: string | null;
  completed_at: string | null;
  batch_id: number | null;
  style_ref_chapter_id: number | null;
}

/**
 * `enqueue_transformation_chapters` 入参。三个上下文数:
 * - `ctx_prev_original` —— 模板 `{{prev_original}}` 占位的前文原文章数
 * - `ctx_prev_transformed` —— 模板 `{{prev_transformed}}` 占位的前文已转换章数
 * (画风参考,不污染原文上下文;若前面没有已转换结果则渲染为 `(暂无已转换参考)`)
 * - `ctx_next_original` —— 模板 `{{next_original}}` 占位的后文原文章数
 * 后端按 (chapter_id, prompt_id, model_config_id) 同时匹配才视为画风参考。
 */
export type EnqueuePayload = {
  transformation_novel_id: number;
  chapter_ids: number[];
  prompt_id: number;
  model_config_id: number;
  ctx_prev_original: number;
  ctx_prev_transformed: number;
  ctx_next_original: number;
};

/**
 * `enqueue_all_chapters` 入参:对 `transformation_novel` 下全部 chapter 入队
 * (后端从 `chapters` 表按 `data_asset_id` 拉全量 chapter_id)。
 */
export type EnqueueAllPayload = Omit<EnqueuePayload, 'chapter_ids'>;

/** `JobQueue` 内部的 job 状态(与 `TransformStatus` 同字面量,但语义层面有别:
 * `TransformStatus` 是 DB 行的持久状态;`JobStatus` 是 worker pool 的内存快照) */
export type JobStatus = 'pending' | 'running' | 'done' | 'failed' | 'cancelled';

/** 单个 job 的实时快照。锁争用时该 job 可能不出现在 snapshot 中。 */
export interface JobInfo {
  transformation_id: number;
  chapter_title: string;
  chapter_idx: number;
  status: JobStatus;
  error: string | null;
  tokens_in: number | null;
  tokens_out: number | null;
}

/** `JobQueue.snapshot()` 一次拉回的全量队列快照,按状态分四组。
 *  锁争用时返回空(字段都为空数组),前端 1s 轮询可不处理。 */
export interface QueueSnapshot {
  pending: JobInfo[];
  running: JobInfo[];
  done: JobInfo[];
  failed: JobInfo[];
}

/// 清洗预览结果。cleaned_text 给前端展示;lines_delta 为输出与输入的行数差
/// (规则折叠/合并短段 → 负数;加缩进不改行数 → 0;现有实现下几乎不会正)。
/// chars_delta 为字符数差(加缩进时为正,合并/折叠时可能为负)。
export interface CleaningPreview {
  cleaned_text: string;
  lines_delta: number;
  chars_delta: number;
}

/**
 * 后端 `prompts` 表行的前端镜像(取自 `nsc_core::models::Prompt`)。
 * `kind` 来自后端 `PromptKind` 枚举(`#[serde(rename_all = "snake_case")]`)
 * —— 前端拿到 / 发回 `"compress"` / `"style"`。
 * - `is_builtin` 为 true 的行在 UI 上不可编辑 / 不可删除,可"复制"成用户版。
 * - `archived = 1` 表示软删 —— 行仍保留供 `transformation_chapters.prompt_id` 反查历史 prompt 名称 / 模板。
 *   默认 list 不返回,需走 `list_prompts_including_archived`。
 */
export interface Prompt {
  id: number;
  name: string;
  kind: 'compress' | 'style';
  template: string;
  is_builtin: boolean;
  /** 0 = 正常,1 = 已归档(软删)。后端 INTEGER 列,前端用 number 收。 */
  archived: number;
}

/**
 * `upsert_prompt` 入参。`id === 0` 表示新建(走 insert);>0 表示更新(走 update)。
 * 字段保持 snake_case-by-default —— `kind` / `name` / `template` 都是单词,
 * 没有 `#[serde(rename_all)]` 在这层 DTO 上,所以前端按字段名原样发。
 * - 排除 `is_builtin` —— 后端不通过此 DTO 改 builtin 标记。
 * - 排除 `archived` —— 软删走 `delete_prompt` / `restore_prompt` 专用命令。
 */
export type PromptInput = Omit<Prompt, 'id' | 'is_builtin' | 'archived'> & { id: number };

/// ai_call_logs 表前端镜像,详见 migrations/0018_ai_call_logs.sql。
/// - business = transform_chapter | test_model(看两条 AI调用 路径)
/// - preview 字段是前 10KB,完整内容看 transformation_chapters.result_content / 调用方上下文
/// - estimated_tokens_in 用 chars/2 启发式(zh-aware 粗估),UI 标注粗估
export type AiCallBusiness = "transform_chapter" | "test_model";
export type AiCallStatus = "success" | "failed";

export interface AiCallLog {
  id: number;
  created_at: string;
  business: AiCallBusiness;
  context_type: string | null;
  context_id: number | null;
  model_config_id: number | null;
  model_name: string;
  base_url: string;
  temperature: number | null;
  max_tokens: number | null;
  system_preview: string | null;
  user_preview: string | null;
  system_size: number;
  user_size: number;
  estimated_tokens_in: number | null;
  actual_tokens_in: number | null;
  actual_tokens_out: number | null;
  status: AiCallStatus;
  response_preview: string | null;
  response_size: number;
  latency_ms: number;
  error: string | null;
}

/** list_ai_call_logs 入参 —— 后端 snake_case DTO,字段保持 Rust 原名。 */
export type AiCallLogFilter = {
  business?: AiCallBusiness | null;
  model_config_id?: number | null;
  status?: AiCallStatus | null;
  limit?: number | null;
};

/** 上传删除前的确认信息。删 upload 不联动删 data_asset，仅提示以供用户另行去处理。 */
export interface UploadDeletePreviewItem {
  id: number;
  title: string;
  chapters_count: number;
  tn_count: number;
}
export interface UploadDeletePreview {
  upload_id: number;
  filename: string;
  source_filename: string;
  derived_data_assets: UploadDeletePreviewItem[];
}
