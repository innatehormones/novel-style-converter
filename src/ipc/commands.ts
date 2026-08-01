// 前端调用规则:
// - 命令参数名(payload / id / chapterId / uploadId 等)由 Tauri 自动 camelCase 化,
//   invoke 外层传参对象用 camelCase 字段名。
// - 内层 DTO 由后端显式 `#[serde(rename_all="snake_case")]` 接收,前端必须按
//   snake_case 原样发(payload.base_url / payload.api_key / ...),不要 inline 改名。
// - 响应类型(后端实体)保持 snake_case,见 types.ts。
//
// Tauri v2: invoke 入口从 `@tauri-apps/api/tauri` 改 `@tauri-apps/api/core`。
import { invoke } from '@tauri-apps/api/core';
import type {
  ModelConfig, ModelConfigInput,
  UploadSummary, CleaningPreview,
  DataAssetChapter, DataAssetRow, CommitDataAssetInput,
  ChapterSegment, ChapterMeta, ChapterContentRow, Chapter, ChapterInput,
  TransformationNovelSummary, TransformationChapterRow,
  EnqueuePayload, EnqueueAllPayload, QueueSnapshot,
} from './types';

// ─── Models ────────────────────────────────────────────────────────────────
export function listModels(): Promise<ModelConfig[]> {
  return invoke<ModelConfig[]>('list_models');
}

export function upsertModel(payload: ModelConfigInput): Promise<number> {
  return invoke<number>('upsert_model', { payload });
}

export function deleteModel(id: number): Promise<void> {
  return invoke<void>('delete_model', { id });
}

export function testModel(payload: ModelConfigInput): Promise<string> {
  return invoke<string>('test_model', { payload });
}

// ─── Uploads ───────────────────────────────────────────────────────────────
export function listUploads(): Promise<UploadSummary[]> {
  return invoke<UploadSummary[]>('list_uploads');
}

export function uploadFile(payload: { file_path: string; filename: string }): Promise<UploadSummary> {
  return invoke<UploadSummary>('upload_file', { payload });
}

export function deleteUpload(id: number): Promise<void> {
  return invoke<void>('delete_upload', { id });
}

export async function getUploadText(id: number): Promise<string> {
  const payload = await invoke<ArrayBuffer | string>('get_upload_text', { id });
  if (typeof payload === 'string') return payload;
  return new TextDecoder().decode(new Uint8Array(payload));
}

export function getUpload(id: number): Promise<UploadSummary> {
  return invoke<UploadSummary>('get_upload', { id });
}

export function updateUploadText(id: number, text: string): Promise<void> {
  return invoke<void>('update_upload_text', { id, text });
}

export function previewCleaning(
  text: string,
  ruleIds: string[],
): Promise<CleaningPreview> {
  return invoke<CleaningPreview>('preview_cleaning', { text, ruleIds });
}

// ─── Data assets ───────────────────────────────────────────────────────────
export function listDataAssetChapters(dataAssetId: number): Promise<DataAssetChapter[]> {
  return invoke<DataAssetChapter[]>('list_data_asset_chapters', { dataAssetId });
}

export function getDataAssetContent(dataAssetId: number): Promise<string> {
  return invoke<string>('get_data_asset_content', { dataAssetId });
}

export function commitDataAsset(
  uploadId: number,
  payload: CommitDataAssetInput,
): Promise<number> {
  return invoke<number>('commit_data_asset', { uploadId, ...payload });
}

/** Upload id → data_asset id(若有)。旧路由重定向用。 */
export function findDataAssetByUpload(uploadId: number): Promise<number | null> {
  return invoke<number | null>('find_data_asset_by_upload', { uploadId });
}

/** 删 data_asset。locked 时拒绝;unlocked 时通过 FK CASCADE 自动清 chapters 等。 */
export function deleteDataAsset(dataAssetId: number): Promise<void> {
  return invoke<void>('delete_data_asset', { dataAssetId });
}

/** Library.vue "数据资产" tab:列出所有 data_asset + 来源 upload 文件名。 */
export function listDataAssets(): Promise<DataAssetRow[]> {
  return invoke<DataAssetRow[]>('list_data_assets');
}

// ─── Chapters ──────────────────────────────────────────────────────────────
export function listChapterSegments(
  uploadId: number,
  markers: number[] | null,
  suppressed: number[] | null,
): Promise<ChapterSegment[]> {
  return invoke<ChapterSegment[]>('list_chapter_segments', {
    uploadId,
    markers,
    suppressed,
  });
}

/** 从 chapters 表读已提交章节段(byte_start/byte_end)。老数据(NULL 范围)被过滤。 */
export function listCommittedSegments(dataAssetId: number): Promise<ChapterSegment[]> {
  return invoke<ChapterSegment[]>('list_committed_segments', { dataAssetId });
}

export function listChapters(dataAssetId: number): Promise<ChapterMeta[]> {
  return invoke<ChapterMeta[]>('list_chapters', { dataAssetId });
}

export function getChapterContents(dataAssetId: number): Promise<ChapterContentRow[]> {
  return invoke<ChapterContentRow[]>('get_chapter_contents', { dataAssetId });
}

export function getChapter(chapterId: number): Promise<Chapter> {
  return invoke<Chapter>('get_chapter', { chapterId });
}

export function parseChapters(dataAssetId: number, segments: ChapterInput[]): Promise<number> {
  return invoke<number>('parse_chapters', { dataAssetId, segments });
}

// ─── Transformation novels ─────────────────────────────────────────────────
export function listTransformationNovels(dataAssetId?: number): Promise<TransformationNovelSummary[]> {
  return invoke<TransformationNovelSummary[]>('list_transformation_novels', { dataAssetId });
}

export function createTransformationNovel(payload: { data_asset_id: number; title: string }): Promise<number> {
  return invoke<number>('create_transformation_novel', { payload });
}

export function updateTransformationNovel(payload: { id: number; title: string }): Promise<void> {
  return invoke<void>('update_transformation_novel', { payload });
}

export function deleteTransformationNovel(id: number): Promise<void> {
  return invoke<void>('delete_transformation_novel', { id });
}

// ─── Transformation chapters ───────────────────────────────────────────────
export function listTransformationChapters(transformationNovelId: number): Promise<TransformationChapterRow[]> {
  return invoke<TransformationChapterRow[]>('list_transformation_chapters', { transformationNovelId });
}

export function listTransformationChaptersForChapter(chapterId: number): Promise<TransformationChapterRow[]> {
  return invoke<TransformationChapterRow[]>('list_transformation_chapters_for_chapter', { chapterId });
}

export function enqueueTransformationChapters(payload: EnqueuePayload): Promise<number[]> {
  return invoke<number[]>('enqueue_transformation_chapters', { payload });
}

export function enqueueAllChapters(payload: EnqueueAllPayload): Promise<number[]> {
  return invoke<number[]>('enqueue_all_chapters', { payload });
}

// ─── Queue ─────────────────────────────────────────────────────────────────
export function getQueueSnapshot(): Promise<QueueSnapshot> {
  return invoke<QueueSnapshot>('get_queue_snapshot');
}