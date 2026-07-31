// 字段命名约定:
// - 响应类型(后端 `#[derive(Serialize)]` 的实体,如 `ModelConfig` / `Upload`)
//   保持 snake_case,与 `crates/nsc-core/src/models/*.rs` 一一对应。
// - IPC 入参的 *命令参数名*(`payload` / `id` 等)由 Tauri 自动 camelCase 化,
//   所以外层 `invoke('cmd', { ... })` 的 key 用 camelCase。
// - IPC 入参的内层 DTO(后端显式 `#[serde(rename_all="snake_case")]` 的)
//   必须按 snake_case 原样发,前端 wrapper 不要做任何 inline 改名。
// 字段变更必须同步修改后端 DTO + 本文件 + commands.ts 中对应 wrapper。

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

export type ModelConfigInput = Omit<ModelConfig, 'id'> & { id: number };

/// State 1: 原始上传文件元数据。不含章节结构(章节在 data_assets)。
export interface UploadSummary {
  id: number;
  sha256: string;
  filename: string;
  byte_size: number;
  uploaded_at: string;
  file_path: string;
}

/// State 2: 一次解析结果 = 一份 data_asset + 一组分章节切片。
export interface DataAssetSummary {
  id: number;
  upload_id: number;
  title: string;
  parsed_at: string;
  locked_at: string | null;
}

/// Library.vue "数据资产" tab 行:data_asset 元数据 + 来源 upload 文件名。
export interface DataAssetRow {
  id: number;
  upload_id: number;
  title: string;
  parsed_at: string;
  locked_at: string | null;
  filename: string;
  byte_size: number;
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

export type ChapterInput = {
  title: string;
  byte_start: number;
  byte_end: number;
};

export interface TransformationNovelSummary {
  id: number;
  data_asset_id: number;
  title: string;
  created_at: string;
  chapters_count: number;
}

export type TransformMode = 'compress' | 'style';
export type TransformStatus = 'pending' | 'running' | 'done' | 'failed' | 'cancelled';

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
}

export type EnqueuePayload = {
  transformation_novel_id: number;
  chapter_ids: number[];
  prompt_id: number;
  model_config_id: number;
  ctx_prev_original: number;
  ctx_prev_transformed: number;
  ctx_next_original: number;
};

export type EnqueueAllPayload = Omit<EnqueuePayload, 'chapter_ids'>;

export type JobStatus = 'pending' | 'running' | 'done' | 'failed' | 'cancelled';

export interface JobInfo {
  transformation_id: number;
  chapter_title: string;
  chapter_idx: number;
  status: JobStatus;
  error: string | null;
  tokens_in: number | null;
  tokens_out: number | null;
}

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
