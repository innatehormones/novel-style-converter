import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import {
  listModels, upsertModel, deleteModel, testModel,
  listUploads, uploadFile, deleteUpload, getUploadText, getUpload,
  previewCleaning,
  listChapterSegments, listCommittedSegments, listChapters, getChapterContents, getChapter, parseChapters,
  listDataAssetChapters as ipcListDataAssetChapters,
  getDataAssetContent as ipcGetDataAssetContent,
  commitDataAsset as ipcCommitDataAsset,
  listTransformationNovels, createTransformationNovel,
  updateTransformationNovel, deleteTransformationNovel,
  listTransformationChapters, enqueueTransformationChapters,
  enqueueAllChapters, getQueueSnapshot,
} from '../ipc/commands';
import type {
  ModelConfig, ModelConfigInput,
  UploadSummary, CleaningPreview, ChapterSegment, ChapterMeta, ChapterContentRow, Chapter, ChapterInput,
  TransformationNovelSummary, TransformationChapterRow,
  QueueSnapshot,
} from '../ipc/types';

const sampleInput: ModelConfigInput = {
  id: 0,
  name: 'new',
  base_url: 'https://x',
  api_key: 'k',
  model: 'm',
  max_tokens: null,
  temperature: null,
  concurrency: 1,
};

describe('Models IPC wrappers', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('listModels calls list_models', async () => {
    const sample: ModelConfig[] = [
      { id: 1, name: 'gpt-4', base_url: 'https://x', api_key: 'k', model: 'gpt-4', max_tokens: 2048, temperature: 0.7, concurrency: 1 },
    ];
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    expect(await listModels()).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('list_models');
  });

  it('upsertModel sends snake_case payload untouched', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(7);
    expect(await upsertModel(sampleInput)).toBe(7);
    expect(invoke).toHaveBeenCalledWith('upsert_model', { payload: sampleInput });
  });

  it('deleteModel sends id', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await deleteModel(3);
    expect(invoke).toHaveBeenCalledWith('delete_model', { id: 3 });
  });

  it('testModel sends snake_case payload untouched', async () => {
    vi.mocked(invoke).mockResolvedValueOnce('pong');
    expect(await testModel(sampleInput)).toBe('pong');
    expect(invoke).toHaveBeenCalledWith('test_model', { payload: sampleInput });
  });
});

describe('Upload IPC wrappers', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('listUploads calls list_uploads', async () => {
    const sample: UploadSummary[] = [
      { id: 1, sha256: 'x', filename: 'A.txt', byte_size: 100, uploaded_at: '2026-07-26T00:00:00Z', file_path: '/x' },
    ];
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    expect(await listUploads()).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('list_uploads');
  });

  it('uploadFile sends snake_case payload with file_path', async () => {
    const sample: UploadSummary = {
      id: 1, sha256: 'x', filename: 'A.txt', byte_size: 3, uploaded_at: '2026-07-26T00:00:00Z', file_path: '/x',
    };
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    const r = await uploadFile({ file_path: 'C:/tmp/A.txt', filename: 'A.txt' });
    expect(r).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('upload_file', { payload: { file_path: 'C:/tmp/A.txt', filename: 'A.txt' } });
  });

  it('deleteUpload sends id', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await deleteUpload(7);
    expect(invoke).toHaveBeenCalledWith('delete_upload', { id: 7 });
  });

  it('getUploadText sends id', async () => {
    vi.mocked(invoke).mockResolvedValueOnce('hello');
    expect(await getUploadText(7)).toBe('hello');
    expect(invoke).toHaveBeenCalledWith('get_upload_text', { id: 7 });
  });

  it('getUpload returns summary', async () => {
    const sample: UploadSummary = {
      id: 1, sha256: 'x', filename: 'A.txt', byte_size: 100, uploaded_at: '2026-07-26T00:00:00Z', file_path: '/x',
    };
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    expect(await getUpload(1)).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('get_upload', { id: 1 });
  });
});

describe('Chapter IPC wrappers', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('listChapterSegments sends markers array', async () => {
    const sample: ChapterSegment[] = [{ title: '第1章', byte_start: 0, byte_end: 10, word_count: 5 }];
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    const r = await listChapterSegments(7, [100, 200], null);
    expect(r).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('list_chapter_segments', {
      uploadId: 7,
      markers: [100, 200],
      suppressed: null,
    });
  });

  it('listChapterSegments sends null markers and suppressed', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listChapterSegments(7, null, null);
    expect(invoke).toHaveBeenCalledWith('list_chapter_segments', {
      uploadId: 7,
      markers: null,
      suppressed: null,
    });
  });

  it('listChapterSegments sends suppressed array', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([{ title: 'A', byte_start: 0, byte_end: 20, word_count: 10 }]);
    await listChapterSegments(7, null, [10]);
    expect(invoke).toHaveBeenCalledWith('list_chapter_segments', {
      uploadId: 7,
      markers: null,
      suppressed: [10],
    });
  });

  it('listChapters calls list_chapters with dataAssetId', async () => {
    const sample: ChapterMeta[] = [{ id: 1, idx: 1, title: 'A', word_count: 5 }];
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    expect(await listChapters(7)).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('list_chapters', { dataAssetId: 7 });
  });

  it('getChapterContents calls get_chapter_contents with dataAssetId', async () => {
    const sample: ChapterContentRow[] = [
      { idx: 1, title: '第1章', content: 'body1' },
      { idx: 2, title: '第2章', content: 'body2' },
    ];
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    expect(await getChapterContents(7)).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('get_chapter_contents', { dataAssetId: 7 });
  });

  it('getChapter calls get_chapter with chapterId', async () => {
    const sample: Chapter = {
      id: 1, data_asset_id: 7, idx: 1, title: 'A', byte_start: 0, byte_end: 10, word_count: 1,
    };
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    expect(await getChapter(1)).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('get_chapter', { chapterId: 1 });
  });

  it('parseChapters sends segments and returns count', async () => {
    const segs: ChapterInput[] = [{ title: 'A', byte_start: 0, byte_end: 10 }];
    vi.mocked(invoke).mockResolvedValueOnce(1);
    expect(await parseChapters(7, segs)).toBe(1);
    expect(invoke).toHaveBeenCalledWith('parse_chapters', { dataAssetId: 7, segments: segs });
  });

  it('listCommittedSegments calls list_committed_segments with dataAssetId', async () => {
    const sample: ChapterSegment[] = [
      { title: '第1章', byte_start: 0, byte_end: 12, word_count: 5 },
      { title: '第2章', byte_start: 12, byte_end: 30, word_count: 8 },
    ];
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    expect(await listCommittedSegments(7)).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('list_committed_segments', { dataAssetId: 7 });
  });
});

describe('data_assets IPC wrappers', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('listDataAssetChapters passes data_asset_id', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    await ipcListDataAssetChapters(7);
    expect(invoke).toHaveBeenCalledWith('list_data_asset_chapters', { dataAssetId: 7 });
  });

  it('getDataAssetContent passes data_asset_id', async () => {
    vi.mocked(invoke).mockResolvedValueOnce('');
    await ipcGetDataAssetContent(7);
    expect(invoke).toHaveBeenCalledWith('get_data_asset_content', { dataAssetId: 7 });
  });

  it('commitDataAsset spreads uploadId and payload', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(99);
    await ipcCommitDataAsset(1, { title: 't', chapters: [] });
    expect(invoke).toHaveBeenCalledWith('commit_data_asset', { uploadId: 1, title: 't', chapters: [] });
  });
});

describe('Transformation novel IPC wrappers', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('listTransformationNovels calls list_transformation_novels', async () => {
    const sample: TransformationNovelSummary[] = [
      { id: 1, data_asset_id: 1, title: 'X', created_at: '2026-07-26T00:00:00Z', chapters_count: 0, default_model_config_id: null, default_prompt_id: null, default_mode: null },
    ];
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    expect(await listTransformationNovels()).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('list_transformation_novels', { dataAssetId: undefined });
  });

  it('listTransformationNovels forwards dataAssetId when provided', async () => {
    const sample: TransformationNovelSummary[] = [];
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    expect(await listTransformationNovels(7)).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('list_transformation_novels', { dataAssetId: 7 });
  });

  it('createTransformationNovel sends snake_case payload', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(7);
    expect(await createTransformationNovel({ data_asset_id: 1, title: 'X' })).toBe(7);
    expect(invoke).toHaveBeenCalledWith('create_transformation_novel', { payload: { data_asset_id: 1, title: 'X' } });
  });

  it('updateTransformationNovel sends snake_case payload', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await updateTransformationNovel({ id: 7, title: 'NEW' });
    expect(invoke).toHaveBeenCalledWith('update_transformation_novel', { payload: { id: 7, title: 'NEW' } });
  });

  it('deleteTransformationNovel sends id', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await deleteTransformationNovel(9);
    expect(invoke).toHaveBeenCalledWith('delete_transformation_novel', { id: 9 });
  });
});

describe('Transformation chapter IPC wrappers', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('listTransformationChapters sends transformationNovelId', async () => {
    const sample: TransformationChapterRow[] = [];
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    expect(await listTransformationChapters(7)).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('list_transformation_chapters', { transformationNovelId: 7 });
  });

  it('enqueueTransformationChapters sends snake_case payload', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([1, 2]);
    expect(await enqueueTransformationChapters({
      transformation_novel_id: 7, chapter_ids: [1, 2], prompt_id: 1, model_config_id: 1,
      ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
    })).toEqual([1, 2]);
    expect(invoke).toHaveBeenCalledWith('enqueue_transformation_chapters', {
      payload: {
        transformation_novel_id: 7, chapter_ids: [1, 2], prompt_id: 1, model_config_id: 1,
        ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
      },
    });
  });

  it('enqueueAllChapters sends snake_case payload', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([1, 2, 3]);
    await enqueueAllChapters({
      transformation_novel_id: 7, prompt_id: 1, model_config_id: 1,
      ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
    });
    expect(invoke).toHaveBeenCalledWith('enqueue_all_chapters', {
      payload: {
        transformation_novel_id: 7, prompt_id: 1, model_config_id: 1,
        ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
      },
    });
  });
});

describe('Queue IPC wrapper', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('getQueueSnapshot calls get_queue_snapshot', async () => {
    const sample: QueueSnapshot = { pending: [], running: [], done: [], failed: [] };
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    expect(await getQueueSnapshot()).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('get_queue_snapshot');
  });
});

describe('Cleaning IPC wrapper', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('previewCleaning sends text and ruleIds', async () => {
    const sample: CleaningPreview = { cleaned_text: '　　A\n', lines_delta: 0, chars_delta: 2 };
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    const r = await previewCleaning('A\n', ['add_indent_to_unindented']);
    expect(r).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('preview_cleaning', {
      text: 'A\n',
      ruleIds: ['add_indent_to_unindented'],
    });
  });
});