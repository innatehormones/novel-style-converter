import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { listDataAssetChapters, getDataAssetContent } from '../ipc/commands';
import type { DataAssetChapter } from '../ipc/types';
import type { ChapterSegment } from '../ipc/types';

/// State 2 读专用 store:从 data_asset 读章节列表 + 原文,纯展示,不允许编辑。
/// selectedContent 按 byte 切片 originalText,避免拉取每章正文。
export const useDataAssetStore = defineStore('dataAsset', () => {
  const dataAssetId = ref<number | null>(null);
  const title = ref<string>('');
  const filename = ref<string>('');
  const parsedAt = ref<string | null>(null);
  const lockedAt = ref<string | null>(null);
  const chapters = ref<ChapterSegment[]>([]);
  const originalText = ref<string>('');
  const selectedIdx = ref<number | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  let requestToken = 0;

  async function load(id: number) {
    dataAssetId.value = id;
    loading.value = true;
    error.value = null;
    chapters.value = [];
    originalText.value = '';
    ++requestToken;
    const token = requestToken;
    try {
      const [chs, content] = await Promise.all([
        listDataAssetChapters(id),
        getDataAssetContent(id),
      ]);
      if (token !== requestToken) return;
      chapters.value = chs.map((c: DataAssetChapter) => ({
        title: c.title,
        byte_start: c.byte_start,
        byte_end: c.byte_end,
        word_count: c.word_count,
        idx: c.idx,
      }));
      originalText.value = content;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      if (token === requestToken) loading.value = false;
    }
  }

  function selectChapter(idx: number) {
    selectedIdx.value = idx;
  }
  function selectFirstIfNone() {
    if (selectedIdx.value == null && chapters.value.length > 0) selectedIdx.value = 0;
  }

  const selectedContent = computed(() => {
    const i = selectedIdx.value;
    if (i == null) return '';
    const c = chapters.value[i];
    if (!c) return '';
    return originalText.value.slice(c.byte_start, c.byte_end);
  });

  return {
    dataAssetId, title, filename, parsedAt, lockedAt,
    chapters, originalText, selectedIdx, selectedContent,
    loading, error,
    load, selectChapter, selectFirstIfNone,
  };
});