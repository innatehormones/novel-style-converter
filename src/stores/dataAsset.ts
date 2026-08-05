import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { listDataAssetChapters, getDataAssetContent, listDataAssets as ipcListDataAssets } from '../ipc/commands';
import type { DataAssetChapter } from '../ipc/types';
import type { ChapterSegment } from '../ipc/types';
import type { DataAssetRow } from '../ipc/types';

/// State 2 读专用 store:从 data_asset 读章节列表 + 原文,纯展示,不允许编辑。
/// selectedContent 按 UTF-8 字节切片 originalText,避免拉取每章正文。
export const useDataAssetStore = defineStore('dataAsset', () => {
  const dataAssetId = ref<number | null>(null);
  const title = ref<string>('');
  const filename = ref<string>('');
  const parsedAt = ref<string | null>(null);
  const tnCount = ref<number>(0);
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
    title.value = '';
    filename.value = '';
    parsedAt.value = null;
    tnCount.value = 0;
    ++requestToken;
    const token = requestToken;
    try {
      const [chs, content, assets] = await Promise.all([
        listDataAssetChapters(id),
        getDataAssetContent(id),
        ipcListDataAssets(),
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
      const row: DataAssetRow | undefined = assets.find((a: DataAssetRow) => a.id === id);
      if (row) {
        title.value = row.title;
        filename.value = row.filename;
        parsedAt.value = row.parsed_at;
        tnCount.value = row.tn_count;
      }
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

  /// byte_start/byte_end 是 UTF-8 字节偏移,不能直接 `String.prototype.slice`
  /// (那是 UTF-16 code unit 索引,中文 UTF-8 是 3 byte/char)。把原文 UTF-8 编码
  /// 后再 subarray 切片、decode 回字符串。等价于后端 Rust 的 `text[s..e]`。
  const originalTextBytes = computed(() => new TextEncoder().encode(originalText.value));
  const selectedContent = computed(() => {
    const i = selectedIdx.value;
    if (i == null) return '';
    const c = chapters.value[i];
    if (!c) return '';
    const bytes = originalTextBytes.value;
    const start = Math.max(0, Math.min(c.byte_start, bytes.length));
    const end = Math.max(start, Math.min(c.byte_end, bytes.length));
    return new TextDecoder().decode(bytes.subarray(start, end));
  });

  return {
    dataAssetId, title, filename, parsedAt, tnCount,
    chapters, originalText, selectedIdx, selectedContent,
    loading, error,
    load, selectChapter, selectFirstIfNone,
  };
});