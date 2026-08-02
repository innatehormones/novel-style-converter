import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import {
  getChapter as ipcGetChapter,
  listChapters as ipcListChapters,
  getDataAssetContent as ipcGetDataAssetContent,
  listTransformationChaptersForChapter as ipcListTnForChapter,
} from '../ipc/commands';
import type { Chapter, ChapterMeta, TransformationChapterRow } from '../ipc/types';

export const useTransformViewStore = defineStore('transformView', () => {
  const chapterId = ref<number | null>(null);
  const dataAssetId = ref<number | null>(null);
  const chapter = ref<Chapter | null>(null);
  const allChapters = ref<ChapterMeta[]>([]);
  const originalText = ref<string>('');
  const transformations = ref<TransformationChapterRow[]>([]);
  const selectedTransformationId = ref<number | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  let requestToken = 0;

  async function load(id: number) {
    chapterId.value = id;
    chapter.value = null;
    allChapters.value = [];
    originalText.value = '';
    transformations.value = [];
    selectedTransformationId.value = null;
    loading.value = true;
    error.value = null;
    ++requestToken;
    const token = requestToken;
    try {
      const ch = await ipcGetChapter(id);
      dataAssetId.value = ch.data_asset_id;
      const [chs, content, tns] = await Promise.all([
        ipcListChapters(ch.data_asset_id),
        ipcGetDataAssetContent(ch.data_asset_id),
        ipcListTnForChapter(id),
      ]);
      if (token !== requestToken) return;
      chapter.value = ch;
      allChapters.value = chs;
      originalText.value = content;
      transformations.value = tns;
      selectedTransformationId.value = tns[0]?.id ?? null;
    } catch (e: unknown) {
      if (token === requestToken) {
        chapter.value = null;
        allChapters.value = [];
        originalText.value = '';
        transformations.value = [];
        selectedTransformationId.value = null;
        error.value = e instanceof Error ? e.message : String(e);
      }
    } finally {
      if (token === requestToken) loading.value = false;
    }
  }

  async function refresh() {
    if (chapterId.value == null) return;
    await load(chapterId.value);
  }

  function selectTransformation(id: number) {
    if (!transformations.value.some((t) => t.id === id)) return;
    selectedTransformationId.value = id;
  }

  const currentIndex = computed(() => {
    const ch = chapter.value;
    if (ch === null) return -1;
    return allChapters.value.findIndex((c) => c.id === ch.id);
  });

  async function gotoChapter(direction: 'prev' | 'next') {
    const currentIdx = currentIndex.value;
    if (currentIdx < 0) return;
    const target = direction === 'prev' ? allChapters.value[currentIdx - 1] : allChapters.value[currentIdx + 1];
    if (!target) return;
    await load(target.id);
  }

  const selectedTransformation = computed(() =>
    transformations.value.find((t) => t.id === selectedTransformationId.value) ?? null,
  );

  /// byte_start/byte_end 是 UTF-8 字节偏移,不能直接 `String.prototype.slice`
  /// (那是 UTF-16 code unit 索引,中文 UTF-8 是 3 byte/char)。把原文 UTF-8 编码
  /// 后再 subarray 切片、decode 回字符串。等价于后端 Rust 的 `text[s..e]`。
  const originalTextBytes = computed(() => new TextEncoder().encode(originalText.value));
  const originalContent = computed(() => {
    const ch = chapter.value;
    if (!ch) return '';
    const bytes = originalTextBytes.value;
    const start = Math.max(0, Math.min(ch.byte_start, bytes.length));
    const end = Math.max(start, Math.min(ch.byte_end, bytes.length));
    return new TextDecoder().decode(bytes.subarray(start, end));
  });

  const canGoPrev = computed(() => currentIndex.value > 0);
  const canGoNext = computed(() => {
    const i = currentIndex.value;
    return i >= 0 && i < allChapters.value.length - 1;
  });

  return {
    chapterId, dataAssetId, chapter, allChapters, originalText,
    transformations, selectedTransformationId, selectedTransformation,
    loading, error, originalContent, canGoPrev, canGoNext,
    load, refresh, selectTransformation, gotoChapter,
  };
});
