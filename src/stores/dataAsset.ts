import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { listDataAssetChapters, listDataAssets as ipcListDataAssets } from '../ipc/commands';
import type { DataAssetChapter } from '../ipc/types';
import type { ChapterSegment } from '../ipc/types';
import type { DataAssetRow } from '../ipc/types';

export const useDataAssetStore = defineStore('dataAsset', () => {
  const dataAssetId = ref<number | null>(null);
  const title = ref<string>('');
  const filename = ref<string>('');
  const parsedAt = ref<string | null>(null);
  const tnCount = ref<number>(0);
  const chapters = ref<ChapterSegment[]>([]);
  const selectedIdx = ref<number | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  /// 跟 chapters 数组对齐:每条对应章节的 source_kind(给 UI 标签用)。
  const sourceKinds = ref<('transformed' | 'original')[]>([]);
  // 工作流转正相关元数据
  const kind = ref<'source' | 'promoted'>('source');
  const sourceWorkflowId = ref<number | null>(null);
  const note = ref<string>('');

  let requestToken = 0;

  async function load(id: number) {
    dataAssetId.value = id;
    loading.value = true;
    error.value = null;
    chapters.value = [];
    sourceKinds.value = [];
    title.value = '';
    filename.value = '';
    parsedAt.value = null;
    tnCount.value = 0;
    kind.value = 'source';
    sourceWorkflowId.value = null;
    note.value = '';
    ++requestToken;
    const token = requestToken;
    try {
      const [chs, assets] = await Promise.all([
        listDataAssetChapters(id),
        ipcListDataAssets(),
      ]);
      if (token !== requestToken) return;
      chapters.value = chs.map((c: DataAssetChapter) => ({
        title: c.title,
        content: c.body,
        word_count: c.word_count,
      }));
      sourceKinds.value = chs.map((c: DataAssetChapter) => c.source_kind);
      const row: DataAssetRow | undefined = assets.find((a: DataAssetRow) => a.id === id);
      if (row) {
        title.value = row.title;
        filename.value = row.filename;
        parsedAt.value = row.parsed_at;
        tnCount.value = row.tn_count;
        kind.value = row.kind;
        sourceWorkflowId.value = row.source_workflow_id;
        note.value = row.note ?? '';
      } else {
        kind.value = 'source';
        sourceWorkflowId.value = null;
        note.value = '';
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

  const selectedContent = computed(() => {
    const i = selectedIdx.value;
    if (i == null) return '';
    return chapters.value[i]?.content ?? '';
  });

  return {
    dataAssetId, title, filename, parsedAt, tnCount,
    chapters, sourceKinds, selectedIdx, selectedContent,
    loading, error,
    kind, sourceWorkflowId, note,
    load, selectChapter, selectFirstIfNone,
  };
});
