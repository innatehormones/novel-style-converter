// @vitest-environment happy-dom
import { setActivePinia, createPinia } from 'pinia';
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { useChaptersStore } from '../stores/chapters';
import type { ChapterSegment } from '../ipc/types';

const sampleSegments: ChapterSegment[] = [
  { title: '第1章', byte_start: 0, byte_end: 10, word_count: 5 },
  { title: '第2章', byte_start: 10, byte_end: 20, word_count: 5 },
];

const freshSegments = (): ChapterSegment[] => sampleSegments.map((s) => ({ ...s }));

/// mock load 流程的并发 IPC:text + committed-segments + upload-meta + (committed 空时) fresh。
/// 实际 invoke 顺序由 Promise.all + 后续 if 决定,但 mock queue 是 FIFO,
/// 所以默认就按这个顺序压栈即可。
function mockLoadPath(segs: ChapterSegment[], opts: { committed?: ChapterSegment[]; text?: string } = {}) {
  vi.mocked(invoke)
    .mockResolvedValueOnce(opts.text ?? 'hello world')
    .mockResolvedValueOnce(opts.committed ?? [])
    .mockResolvedValueOnce({ id: 7, sha256: 'x', filename: 'A.txt', byte_size: 100, uploaded_at: '2026-07-26T00:00:00Z', file_path: '/x' })
    .mockResolvedValueOnce(segs);
}

describe('chapters store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(invoke).mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('load 取 text + committed + fresh,committed 空则用 fresh 且无 edits 时不跑 splitter', async () => {
    const segs = freshSegments();
    mockLoadPath(segs);
    const store = useChaptersStore();
    await store.load(7);
    // load 期间并发取 text + committed-segments + upload-meta(committed 空),
    // 再走 fresh splitter。applyWorking 在 markers/suppressed 都空时不调 splitter,直接用 source。
    expect(invoke).toHaveBeenCalledWith('get_upload_text', { id: 7 });
    expect(invoke).toHaveBeenCalledWith('list_committed_segments', { dataAssetId: 7 });
    expect(invoke).toHaveBeenCalledWith('get_upload', { id: 7 });
    expect(invoke).toHaveBeenCalledWith('list_chapter_segments', { uploadId: 7, markers: [], suppressed: [] });
    expect(store.rawText).toBe('hello world');
    expect(store.source).toEqual(segs);
    expect(store.workingChapters).toEqual(segs);
    expect(store.sourceKind).toBe('fresh');
    expect(store.committed).toBe(false);
    expect(store.dirty).toBe(false);
    expect(store.uploadId).toBe(7);
    expect(store.loading).toBe(false);
  });

  it('committed 非空时 sourceKind=committed 且不跑 fresh splitter', async () => {
    const committedSegs: ChapterSegment[] = [
      { title: '序章', byte_start: 0, byte_end: 5, word_count: 2 },
      { title: '卷一', byte_start: 5, byte_end: 30, word_count: 10 },
    ];
    mockLoadPath(freshSegments(), { committed: committedSegs });
    const store = useChaptersStore();
    await store.load(7);
    expect(store.sourceKind).toBe('committed');
    expect(store.committed).toBe(true);
    expect(store.source).toEqual(committedSegs);
    expect(store.workingChapters).toEqual(committedSegs);
  });

  it('committed 模式下 dirty 默认 false;改标题变 true', async () => {
    const committedSegs: ChapterSegment[] = [
      { title: '序章', byte_start: 0, byte_end: 5, word_count: 2 },
    ];
    mockLoadPath(freshSegments(), { committed: committedSegs });
    const store = useChaptersStore();
    await store.load(7);
    expect(store.dirty).toBe(false);

    store.updateTitle(0, '改了');
    expect(store.dirty).toBe(true);
    expect(store.workingChapters[0].title).toBe('改了');
  });

  it('commit 后 sourceKind=committed 且 dirty=false;edits 清空', async () => {
    mockLoadPath(freshSegments());
    const store = useChaptersStore();
    await store.load(7);
    store.addMarker(100);
    expect(store.markers).toEqual([100]);

    vi.mocked(invoke).mockResolvedValueOnce(99);
    const newId = await store.commit('My Novel');
    expect(newId).toBe(99);
    expect(invoke).toHaveBeenCalledWith('commit_data_asset', {
      uploadId: 7,
      title: 'My Novel',
      chapters: [
        { title: '第1章', byte_start: 0, byte_end: 10 },
        { title: '第2章', byte_start: 10, byte_end: 20 },
      ],
    });
    expect(store.sourceKind).toBe('committed');
    expect(store.dirty).toBe(false);
    expect(store.markers).toEqual([]);
    expect(store.suppressed).toEqual([]);
  });

  it('reSplit 把 committed source 退回 fresh', async () => {
    const committedSegs: ChapterSegment[] = [
      { title: '序章', byte_start: 0, byte_end: 5, word_count: 2 },
    ];
    mockLoadPath(freshSegments(), { committed: committedSegs });
    const store = useChaptersStore();
    await store.load(7);
    expect(store.sourceKind).toBe('committed');

    // committed 非空时 mockLoadPath 的第 4 个 mock(初始 fresh splitter)没人消费,
    // 会留在队列里。reSplit 直接 push 会让它先被吃掉,所以这里 reset 一次。
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValueOnce([{ title: '第一章', byte_start: 0, byte_end: 12, word_count: 8 }]);
    await store.reSplit();
    expect(store.sourceKind).toBe('fresh');
    expect(store.source).toEqual([{ title: '第一章', byte_start: 0, byte_end: 12, word_count: 8 }]);
  });

  it('addMarker 去重 + 200ms 后触发 list_chapter_segments', async () => {
    vi.useFakeTimers();
    const segs = freshSegments();
    mockLoadPath(segs);
    const store = useChaptersStore();
    await store.load(7);
    vi.mocked(invoke).mockClear();

    store.addMarker(100);
    store.addMarker(100);
    store.addMarker(50);

    expect(store.markers).toEqual([50, 100]);

    expect(invoke).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(250);
    expect(invoke).toHaveBeenCalledWith('list_chapter_segments', { uploadId: 7, markers: [50, 100], suppressed: [] });
  });

  it('removeMarker 后 edits 空 → 不再走 splitter,回 source', async () => {
    vi.useFakeTimers();
    mockLoadPath(freshSegments());
    const store = useChaptersStore();
    await store.load(7);
    vi.mocked(invoke).mockClear();

    store.addMarker(100);
    await vi.advanceTimersByTimeAsync(250);
    vi.mocked(invoke).mockClear();

    store.removeMarker(100);
    await vi.advanceTimersByTimeAsync(250);

    expect(invoke).not.toHaveBeenCalled();
    expect(store.workingChapters).toEqual(freshSegments());
  });

  it('updateTitle 就地更新 workingChapters + dirty', async () => {
    mockLoadPath(freshSegments());
    const store = useChaptersStore();
    await store.load(7);

    store.updateTitle(0, '新标题');

    expect(store.workingChapters[0].title).toBe('新标题');
    expect(store.workingChapters[1].title).toBe('第2章');
    expect(store.dirty).toBe(true);
  });

  it('updateTitle 改回原值清掉 override 且 dirty=false', async () => {
    mockLoadPath(freshSegments());
    const store = useChaptersStore();
    await store.load(7);

    store.updateTitle(0, '新标题');
    expect(store.dirty).toBe(true);
    store.updateTitle(0, '第1章');
    expect(store.dirty).toBe(false);
    expect(store.workingChapters[0].title).toBe('第1章');
  });

  it('updateTitle on different chapters keeps both independent (回归: 用户报告「选第二章过去,第一章名字变回去」)', async () => {
    mockLoadPath(freshSegments());
    const store = useChaptersStore();
    await store.load(7);

    store.updateTitle(0, '新第一章');
    store.updateTitle(1, '新第二章');

    expect(store.workingChapters[0].title).toBe('新第一章');
    expect(store.workingChapters[1].title).toBe('新第二章');
    expect(store.titleOverrides[store.workingChapters[0].byte_start]).toBe('新第一章');
    expect(store.titleOverrides[store.workingChapters[1].byte_start]).toBe('新第二章');
  });

  it('reset 清空 markers + suppressed + overrides → 回 source,不调 splitter', async () => {
    vi.useFakeTimers();
    mockLoadPath(freshSegments());
    const store = useChaptersStore();
    await store.load(7);
    store.addMarker(100);
    await vi.advanceTimersByTimeAsync(250);
    vi.mocked(invoke).mockClear();

    store.reset();
    expect(store.markers).toEqual([]);
    expect(store.suppressed).toEqual([]);
    expect(store.dirty).toBe(false);
    await vi.advanceTimersByTimeAsync(250);

    expect(invoke).not.toHaveBeenCalled();
    expect(store.workingChapters).toEqual(freshSegments());
  });

  it('commit 发 workingChapters 返回 data_asset_id', async () => {
    mockLoadPath(freshSegments());
    const store = useChaptersStore();
    await store.load(7);

    vi.mocked(invoke).mockResolvedValueOnce(99);
    const newId = await store.commit('My Novel');

    expect(invoke).toHaveBeenCalledWith('commit_data_asset', {
      uploadId: 7,
      title: 'My Novel',
      chapters: [
        { title: '第1章', byte_start: 0, byte_end: 10 },
        { title: '第2章', byte_start: 10, byte_end: 20 },
      ],
    });
    expect(newId).toBe(99);
  });

  it('commit failure sets error and rethrows', async () => {
    mockLoadPath(freshSegments());
    const store = useChaptersStore();
    await store.load(7);

    const boom = new Error('commit 炸了');
    vi.mocked(invoke).mockRejectedValueOnce(boom);

    await expect(store.commit('My Novel')).rejects.toBe(boom);
    expect(store.error).toBe('commit 炸了');
  });

  it('removeChapter merges into previous via suppressed', async () => {
    vi.useFakeTimers();
    mockLoadPath(freshSegments());
    const store = useChaptersStore();
    await store.load(7);
    vi.mocked(invoke).mockClear();

    store.removeChapter(1);
    expect(store.suppressed).toEqual([10]);

    await vi.advanceTimersByTimeAsync(250);
    expect(invoke).toHaveBeenCalledWith('list_chapter_segments', {
      uploadId: 7,
      markers: [],
      suppressed: [10],
    });
  });

  it('removeChapter on first chapter is a noop', async () => {
    vi.useFakeTimers();
    mockLoadPath(freshSegments());
    const store = useChaptersStore();
    await store.load(7);
    vi.mocked(invoke).mockClear();

    store.removeChapter(0);
    expect(store.suppressed).toEqual([]);
    await vi.advanceTimersByTimeAsync(250);
    expect(invoke).not.toHaveBeenCalled();
  });

  it('setSearchQuery updates query and resets currentHitIndex to 0', () => {
    vi.mocked(invoke).mockReset();
    const store = useChaptersStore();
    store.currentHitIndex = 3;
    store.setSearchQuery('hello');
    expect(store.searchQuery).toBe('hello');
    expect(store.currentHitIndex).toBe(0);
  });

  it('nextSearchHit wraps from last to 0', () => {
    vi.mocked(invoke).mockReset();
    const store = useChaptersStore();
    store.currentHitIndex = 2;
    store.nextSearchHit(3);
    expect(store.currentHitIndex).toBe(0);
  });

  it('prevSearchHit wraps from 0 to total-1', () => {
    vi.mocked(invoke).mockReset();
    const store = useChaptersStore();
    store.currentHitIndex = 0;
    store.prevSearchHit(3);
    expect(store.currentHitIndex).toBe(2);
  });

  it('load resets searchQuery and currentHitIndex', async () => {
    mockLoadPath(freshSegments());
    const store = useChaptersStore();
    store.setSearchQuery('hello');
    store.currentHitIndex = 2;
    await store.load(7);
    expect(store.searchQuery).toBe('');
    expect(store.currentHitIndex).toBe(0);
  });

  it('addMarker at suppressed position clears suppression', async () => {
    vi.useFakeTimers();
    mockLoadPath(freshSegments());
    const store = useChaptersStore();
    await store.load(7);
    store.removeChapter(1);
    await vi.advanceTimersByTimeAsync(250);
    expect(store.suppressed).toEqual([10]);
    vi.mocked(invoke).mockClear();

    store.addMarker(10);
    expect(store.suppressed).toEqual([]);
    expect(store.markers).toEqual([10]);

    await vi.advanceTimersByTimeAsync(250);
    expect(invoke).toHaveBeenCalledWith('list_chapter_segments', {
      uploadId: 7,
      markers: [10],
      suppressed: [],
    });
  });

  it('loading a different upload synchronously clears visible data so the previous novel does not flash', async () => {
    // 先载入小说 A,确认有数据。
    mockLoadPath(freshSegments());
    const store = useChaptersStore();
    await store.load(7);
    expect(store.workingChapters.length).toBeGreaterThan(0);
    expect(store.rawText).toBe('hello world');

    // 切到小说 B:用永不 resolve 的 invoke 模拟 IPC 阻塞,
    // 此时 load 内的清空应该已经同步生效,不能再渲染小说 A 的数据。
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockReturnValue(new Promise(() => {}));
    void store.load(8);

    expect(store.uploadId).toBe(8);
    expect(store.workingChapters).toEqual([]);
    expect(store.source).toEqual([]);
    expect(store.sourceKind).toBeNull();
    expect(store.rawText).toBe('');
    expect(store.filename).toBe('');
    expect(store.loading).toBe(true);
  });
});