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
    // '第一章 山村少年\n' 是 9 字符 / 23 字节(每中文字符 3 字节 UTF-8,空格 + 换行 1 字节)。
    // byte_start/byte_end 在 UTF-8 字节坐标系。
    const fullText = '第一章 山村少年\n';
    const fullBytes = new TextEncoder().encode(fullText).length;
    const splitSegs = [
      { title: '第一章 山村少年', byte_start: 0, byte_end: fullBytes, word_count: 8 },
    ];

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case 'get_upload_text':
          return fullText;
        case 'list_committed_segments':
          return [];
        case 'get_upload':
          return { id: 7, sha256: 'x', filename: 'novel.txt', byte_size: fullBytes, uploaded_at: '2026-07-29T00:00:00Z', file_path: '/x' };
        case 'list_chapter_segments':
          return splitSegs;
        case 'commit_data_asset':
          return 42; // 新 data_asset_id
        case 'list_data_asset_chapters':
          return [
            { id: 1, idx: 0, title: '第一章 山村少年', byte_start: 0, byte_end: fullBytes, word_count: 8 },
          ];
        case 'get_data_asset_content':
          return fullText;
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
      chapters: [{ title: '第一章 山村少年', byte_start: 0, byte_end: fullBytes }],
    });

    // Step 3: 跳到 DataAsset.vue,加载新 data_asset
    const dataStore = useDataAssetStore();
    await dataStore.load(42);
    expect(dataStore.chapters).toHaveLength(1);
    expect(dataStore.chapters[0].title).toBe('第一章 山村少年');
    expect(dataStore.chapters[0].byte_start).toBe(0);
    expect(dataStore.chapters[0].byte_end).toBe(fullBytes);

    // Step 4: 默认选第一章节,正文切片(按 UTF-8 字节)
    dataStore.selectFirstIfNone();
    expect(dataStore.selectedIdx).toBe(0);
    expect(dataStore.selectedContent).toBe(fullText);
  });

  it('多章节中文小说:选 ch1 只显示 ch1(回归 String.slice 把字节当字符的 bug)', async () => {
    // 6 章节 + 中文 + ASCII 混合:每中文字符 3 byte,空格/换行 1 byte。
    // 原 bug:String.prototype.slice(byte_start, byte_end) 把字节当 UTF-16 字符索引用,
    // 导致 ch1 实际显示 ch1+ch2+ch3 内容(短文本时甚至吞掉后面章节)。
    const text =
      '第一章 山村少年\n小明起床。\n' +
      '第二章 出门\n他去砍柴。\n' +
      '第三章 又一天\n太阳升起。\n' +
      '第四章 远行\n走到镇上。\n' +
      '第五章 集市\n买了些米。\n' +
      '第六章 归家\n回家吃饭。';
    const bytes = new TextEncoder().encode(text);

    // 用 TextEncoder 在 UTF-8 字节流中找每章起点,模拟 splitter 输出
    function byteIndexOf(needle: string): number {
      const needleBytes = new TextEncoder().encode(needle);
      outer: for (let i = 0; i <= bytes.length - needleBytes.length; i++) {
        for (let j = 0; j < needleBytes.length; j++) {
          if (bytes[i + j] !== needleBytes[j]) continue outer;
        }
        return i;
      }
      throw new Error(`not found: ${needle}`);
    }
    const titles = ['第一章', '第二章', '第三章', '第四章', '第五章', '第六章'];
    const chList = titles.map((t, i) => {
      const s = byteIndexOf(t);
      const e = i + 1 < titles.length ? byteIndexOf(titles[i + 1]!) : bytes.length;
      return { id: i + 1, idx: i, title: t, byte_start: s, byte_end: e, word_count: 0 };
    });

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'list_data_asset_chapters') return chList;
      if (cmd === 'get_data_asset_content') return text;
      throw new Error(`unexpected cmd: ${cmd}`);
    });

    const store = useDataAssetStore();
    await store.load(1);
    store.selectChapter(0);
    expect(store.selectedContent).toBe('第一章 山村少年\n小明起床。\n');
    store.selectChapter(1);
    expect(store.selectedContent).toBe('第二章 出门\n他去砍柴。\n');
    store.selectChapter(5);
    expect(store.selectedContent).toBe('第六章 归家\n回家吃饭。');
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