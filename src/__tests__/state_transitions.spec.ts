// @vitest-environment happy-dom
import { setActivePinia, createPinia } from 'pinia';
import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { useChaptersStore } from '../stores/chapters';
import { useDataAssetStore } from '../stores/dataAsset';

describe('State 1 → 2 transition via parse.vue → DataAsset.vue', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(invoke).mockReset();
  });

  it('parse wizard commits → dataAsset loads same chapters + content', async () => {
    // parse.vue 的章节切片结果
    const splitSegs = [
      { title: '第一章 山村少年', byte_start: 0, byte_end: 18, word_count: 8 },
    ];

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case 'get_upload_text':
          return '第一章 山村少年\n';
        case 'list_committed_segments':
          return [];
        case 'get_upload':
          return { id: 7, sha256: 'x', filename: 'novel.txt', byte_size: 18, uploaded_at: '2026-07-29T00:00:00Z', file_path: '/x' };
        case 'list_chapter_segments':
          return splitSegs;
        case 'commit_data_asset':
          return 42; // 新 data_asset_id
        case 'list_data_asset_chapters':
          return [
            { id: 1, idx: 0, title: '第一章 山村少年', byte_start: 0, byte_end: 18, word_count: 8 },
          ];
        case 'get_data_asset_content':
          return '第一章 山村少年\n';
        default:
          throw new Error(`unexpected cmd: ${cmd}`);
      }
    });

    // Step 1: parse wizard 加载 upload 并切片
    const chaptersStore = useChaptersStore();
    await chaptersStore.load(7);
    expect(chaptersStore.workingChapters).toHaveLength(1);
    expect(chaptersStore.workingChapters[0].title).toBe('第一章 山村少年');

    // Step 2: 用户 commit
    const newId = await chaptersStore.commit('第一卷');
    expect(newId).toBe(42);
    expect(invoke).toHaveBeenCalledWith('commit_data_asset', {
      uploadId: 7,
      title: '第一卷',
      chapters: [{ title: '第一章 山村少年', byte_start: 0, byte_end: 18 }],
    });

    // Step 3: 跳到 DataAsset.vue,加载新 data_asset
    const dataStore = useDataAssetStore();
    await dataStore.load(42);
    expect(dataStore.chapters).toHaveLength(1);
    expect(dataStore.chapters[0].title).toBe('第一章 山村少年');
    expect(dataStore.chapters[0].byte_start).toBe(0);
    expect(dataStore.chapters[0].byte_end).toBe(18);

    // Step 4: 默认选第一章节,正文切片
    dataStore.selectFirstIfNone();
    expect(dataStore.selectedIdx).toBe(0);
    expect(dataStore.selectedContent).toBe('第一章 山村少年\n');
  });

  it('locked data_asset 不再 commit 同一 upload', async () => {
    // simulate: 已经有 data_asset,parse wizard 试图再次 commit 应被后端拒绝
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'commit_data_asset') {
        throw new Error('upload 7 已有 data_asset,无法重复提交');
      }
      return [];
    });
    const chaptersStore = useChaptersStore();
    await chaptersStore.load(7);
    await expect(chaptersStore.commit('dup')).rejects.toThrow('已有 data_asset');
  });
});