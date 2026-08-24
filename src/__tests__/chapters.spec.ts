// Pinia store behaviour for the parse page chapter list.
// Covers the round-trip bug where clicking 章 on a previously-merged
// chapter title line did not bring that chapter back into the left list.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';

// Mock IPC commands consumed by the chapters store. We only stub what load()
// actually calls; nothing else in this file touches Tauri.
vi.mock('../ipc/commands', () => ({
  getUploadText: vi.fn(async () => TEXT),
  getUpload: vi.fn(async (id) => ({ id, filename: 'sample.txt', size: TEXT.length })),
  findDataAssetByUpload: vi.fn(async () => []),
  listChapterSegments: vi.fn(async () => SEGMENTS),
  listCommittedSegments: vi.fn(async () => []),
  commitDataAsset: vi.fn(async () => 1),
}));

// Mock @vueuse/core so the debounce fires synchronously inside tests;
// otherwise debouncedRecompute leaves the store in a stale state.
vi.mock('@vueuse/core', () => ({
  useDebounceFn: (fn: (...args: unknown[]) => unknown) => fn,
}));

import { useChaptersStore } from '../stores/chapters';

const TEXT = "content intro\n\n第一章：开篇\nbody1 line 1\nbody1 line 2\nbody1 line 3\n第二章今世只想生孩子\nbody2 line 1\nbody2 line 2\nbody2 line 3\n第三章：误会\nbody3 line 1\nbody3 line 2";

// Line index for the chapter 2 title in TEXT above (0-based).
const CH2_TITLE_LINE = 6;  // title line of Ch2 with 3-line Ch1 body

const SEGMENTS = [
  { title: '第一章：开篇', content: 'body1 line 1\nbody1 line 2\nbody1 line 3', word_count: 6 },
  { title: '第二章今世只想生孩子', content: 'body2 line 1\nbody2 line 2\nbody2 line 3', word_count: 6 },
  { title: '第三章：误会', content: 'body3 line 1\nbody3 line 2', word_count: 6 },
];

beforeEach(() => {
  setActivePinia(createPinia());
});

afterEach(() => {
  vi.clearAllMocks();
});

describe('parse store: 章 click restores a merged chapter', () => {
  it('addMarker on a merged chapter title line brings that chapter back', async () => {
    const store = useChaptersStore();
    await store.load(1);

    expect(store.workingChapters.map((c) => c.title)).toEqual([
      '第一章：开篇',
      '第二章今世只想生孩子',
      '第三章：误会',
    ]);

    // User clicks 并入上一章 on chapter 2.
    store.removeChapter(1);

    expect(store.workingChapters.map((c) => c.title)).toEqual([
      '第一章：开篇',
      '第三章：误会',
    ]);

    // User clicks 章 on chapter 2 title row. The gutter translates 1-based
    // CM6 line numbers to 0-based store keys via addMarker.
    store.addMarker(String(CH2_TITLE_LINE));

    // The chapter must come back. Pre-fix this was broken: addMarker did
    // suppressed.value.includes(key) where key is a line-number string and
    // suppressed is seg.content - they never matched, so un-suppress never
    // ran; splitChaptersByMarkers also drops markers at chapter boundary
    // lines, so the chapter stayed merged.
    expect(store.workingChapters.map((c) => c.title)).toEqual([
      '第一章：开篇',
      '第二章今世只想生孩子',
      '第三章：误会',
    ]);
  });

  it('addMarker on a non-title body line splits that chapter into two', async () => {
    const store = useChaptersStore();
    await store.load(1);

    // Line 4 is body1 line 2, which is inside Ch1's body (start=3, end=5).
    // The line is short enough that parseChapterTitle treats it as a title
    // candidate, so the upper part is "body1 line 1" and the lower part is
    // titled by the marker line. Either way the chapter count goes up.
    store.addMarker('4');

    expect(store.markers).toContain('4');
    expect(store.suppressed).toEqual([]);
    // Ch1 is split, Ch2 and Ch3 stay => 4 chapters.
    expect(store.workingChapters.length).toBeGreaterThan(3);
  });
});
