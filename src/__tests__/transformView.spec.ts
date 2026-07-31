// @vitest-environment happy-dom
import { setActivePinia, createPinia } from 'pinia';
import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { useTransformViewStore } from '../stores/transformView';
import type { Chapter, ChapterMeta, TransformationChapterRow } from '../ipc/types';

const sampleChapter: Chapter = {
  id: 7, data_asset_id: 1, idx: 0, title: '第1章', byte_start: 0, byte_end: 100, word_count: 50,
};
const sampleMeta: ChapterMeta[] = [
  sampleChapter,
  { id: 8, idx: 1, title: '第2章', word_count: 30 },
];
const sampleContent = 'X'.repeat(200);
const sampleTn: TransformationChapterRow[] = [
  {
    id: 100, transformation_novel_id: 1, chapter_id: 7, chapter_idx: 0, chapter_title: '第1章',
    mode: 'style', prompt_id: 1, model_config_id: 1, status: 'done',
    result_content: 'trans-X', tokens_in: 10, tokens_out: 5, error: null,
    started_at: null, completed_at: null,
  },
];

describe('transformView store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(invoke).mockReset();
  });

  it('load 并发调 4 IPC,失败原子清空', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_chapter') return Promise.resolve(sampleChapter);
      if (cmd === 'list_chapters') return Promise.resolve(sampleMeta);
      if (cmd === 'get_data_asset_content') return Promise.resolve(sampleContent);
      if (cmd === 'list_transformation_chapters_for_chapter') return Promise.resolve(sampleTn);
      return Promise.reject(new Error(`unexpected: ${cmd}`));
    });
    const s = useTransformViewStore();
    await s.load(7);
    expect(s.chapterId).toBe(7);
    expect(s.dataAssetId).toBe(1);
    expect(s.chapter).toEqual(sampleChapter);
    expect(s.allChapters).toEqual(sampleMeta);
    expect(s.originalText).toBe(sampleContent);
    expect(s.transformations).toEqual(sampleTn);
    expect(s.selectedTransformationId).toBe(100);
    expect(s.error).toBeNull();
    expect(s.loading).toBe(false);
  });

  it('任一 IPC 失败 → 重置并报错', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_chapter') return Promise.resolve(sampleChapter);
      if (cmd === 'list_chapters') return Promise.resolve(sampleMeta);
      if (cmd === 'get_data_asset_content') return Promise.reject(new Error('boom'));
      if (cmd === 'list_transformation_chapters_for_chapter') return Promise.resolve(sampleTn);
      return Promise.reject(new Error(`unexpected: ${cmd}`));
    });
    const s = useTransformViewStore();
    await s.load(7);
    expect(s.error).toBe('boom');
    expect(s.chapter).toBeNull();
    expect(s.allChapters).toEqual([]);
    expect(s.originalText).toBe('');
    expect(s.transformations).toEqual([]);
    expect(s.loading).toBe(false);
  });

  it('gotoChapter 触发重 load', async () => {
    let lastChapterId: number | undefined;
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === 'get_chapter') {
        lastChapterId = (args as { chapterId: number })?.chapterId;
        return Promise.resolve({ ...sampleChapter, id: lastChapterId! });
      }
      if (cmd === 'list_chapters') return Promise.resolve(sampleMeta);
      if (cmd === 'get_data_asset_content') return Promise.resolve(sampleContent);
      if (cmd === 'list_transformation_chapters_for_chapter') return Promise.resolve([]);
      return Promise.reject(new Error(`unexpected: ${cmd}`));
    });
    const s = useTransformViewStore();
    await s.load(7);
    expect(lastChapterId).toBe(7);
    await s.gotoChapter('next');
    expect(lastChapterId).toBe(8);
  });

  it('selectTransformation 改 id,无效 id 被忽略', () => {
    const s = useTransformViewStore();
    s.transformations = sampleTn as any;
    s.selectTransformation(100);
    expect(s.selectedTransformationId).toBe(100);
    s.selectTransformation(999);
    expect(s.selectedTransformationId).toBe(100);
  });

  it('refresh 重 load 当前 chapterId', async () => {
    let calls = 0;
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_chapter') { calls += 1; return Promise.resolve(sampleChapter); }
      if (cmd === 'list_chapters') return Promise.resolve(sampleMeta);
      if (cmd === 'get_data_asset_content') return Promise.resolve(sampleContent);
      if (cmd === 'list_transformation_chapters_for_chapter') return Promise.resolve([]);
      return Promise.reject(new Error(`unexpected: ${cmd}`));
    });
    const s = useTransformViewStore();
    await s.load(7);
    const before = calls;
    await s.refresh();
    expect(calls).toBe(before + 1);
  });

  it('originalContent 按 byte 切片 current chapter 范围', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_chapter') return Promise.resolve({ ...sampleChapter, byte_start: 10, byte_end: 110 });
      if (cmd === 'list_chapters') return Promise.resolve(sampleMeta);
      if (cmd === 'get_data_asset_content') return Promise.resolve('0123456789ABCDEFGHIJ');
      if (cmd === 'list_transformation_chapters_for_chapter') return Promise.resolve([]);
      return Promise.reject(new Error(`unexpected: ${cmd}`));
    });
    const s = useTransformViewStore();
    await s.load(7);
    expect(s.originalContent).toBe('ABCDEFGHIJ');
  });

  it('selectedTransformation 计算属性随 selectedTransformationId 变化', async () => {
    const tn2: TransformationChapterRow = {
      ...sampleTn[0]!, id: 101, result_content: 'trans-Y',
    };
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_chapter') return Promise.resolve(sampleChapter);
      if (cmd === 'list_chapters') return Promise.resolve(sampleMeta);
      if (cmd === 'get_data_asset_content') return Promise.resolve(sampleContent);
      if (cmd === 'list_transformation_chapters_for_chapter') return Promise.resolve([sampleTn[0]!, tn2]);
      return Promise.reject(new Error(`unexpected: ${cmd}`));
    });
    const s = useTransformViewStore();
    await s.load(7);
    expect(s.selectedTransformation?.id).toBe(100);
    s.selectTransformation(101);
    expect(s.selectedTransformation?.id).toBe(101);
  });

  it('canGoPrev/canGoNext 在边界为 false', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === 'get_chapter') {
        const id = (args as { chapterId: number })?.chapterId ?? 7;
        return Promise.resolve({ ...sampleChapter, id });
      }
      if (cmd === 'list_chapters') return Promise.resolve(sampleMeta);
      if (cmd === 'get_data_asset_content') return Promise.resolve(sampleContent);
      if (cmd === 'list_transformation_chapters_for_chapter') return Promise.resolve([]);
      return Promise.reject(new Error(`unexpected: ${cmd}`));
    });
    const s = useTransformViewStore();
    await s.load(7);
    expect(s.canGoPrev).toBe(false);
    expect(s.canGoNext).toBe(true);
    await s.gotoChapter('next');
    expect(s.canGoPrev).toBe(true);
    expect(s.canGoNext).toBe(false);
  });
});
