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
 * - `concurrency` 当前未使用,保留给后续 per-model 限流;前端不要读它做行为判断
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
}

/**
 * `upsert_model` / `test_model` 入参:`id === 0` 表示新建,否则按 id 更新。
 * 这是后端 snake_case DTO(内层字段原样发,不要 inline 改名)。
 */
export type ModelConfigInput = Omit<ModelConfig, 'id'> & { id: number };

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

/// State 2: 一次解析结果 = 一份 data_asset + 一组分章节切片。
export interface DataAssetSummary {
  id: number;
  upload_id: number;
  title: string;
  parsed_at: string;
  /// COUNT(transformation_novels.id) WHERE data_asset_id = da.id。
  /// 前端按钮禁用按这个走:>0 表示有工作区引用,不允许随便删。
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
}

/// State 2 章节元数据(从 list_data_asset_chapters 返回)。正文由前端按 byte 切片 original_text。
export interface DataAssetChapter {
  id: number;
  idx: number;
  title: string;
  byte_start: number;
  byte_end: number;
  word_count: number;
}

/// commit_data_asset 入参:title + 章节列表(每个含 title + byte 范围)。
export interface CommitDataAssetInput {
  title: string;
  chapters: Array<{
    title: string;
    byte_start: number;
    byte_end: number;
  }>;
}

export interface ChapterSegment {
  title: string;
  byte_start: number;
  byte_end: number;
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
  byte_start: number;
  byte_end: number;
  word_count: number;
}

/**
 * `commit_data_asset` / `parse_chapters` 入参的章节元素:
 * 仅标题 + byte 范围。后端按 byte range 切片原文计算 `word_count` / `idx`。
 */
export type ChapterInput = {
  title: string;
  byte_start: number;
  byte_end: number;
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
  default_mode: TransformMode | null;
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
  default_mode?: TransformMode | null;
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
  default_mode?: TransformMode | null;
}

/** 转换模式:`compress` = 内容压缩,`style` = 文风转换。prompt.kind 必须与此对齐。 */
export type TransformMode = 'compress' | 'style';

// === Workflow 工作流 ===
/** 后端 `BatchStatus` 收敛到两态:`running` / `stopped`(spec §3.3)。Stopped 后只能 retry 空槽。 */
export type WorkflowStatus = 'running' | 'stopped';

/**
 * `list_workflows` / `get_workflow` 返回:工作流汇总 + 章节计数。
 * counts 直接嵌在行内 —— 不用单独调 count 接口。
 */
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
  mode: TransformMode;
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
 * `is_builtin` 为 true 的行在 UI 上不可编辑、不可删除,可"复制"成用户版。
 */
export interface Prompt {
  id: number;
  name: string;
  kind: 'compress' | 'style';
  template: string;
  is_builtin: boolean;
}

/**
 * `upsert_prompt` 入参。`id === 0` 表示新建(走 insert);>0 表示更新(走 update)。
 * 字段保持 snake_case-by-default —— `kind` / `name` / `template` 都是单词,
 * 没有 `#[serde(rename_all)]` 在这层 DTO 上,所以前端按字段名原样发。
 */
export type PromptInput = Omit<Prompt, 'id' | 'is_builtin'> & { id: number };
