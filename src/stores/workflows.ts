import { defineStore } from 'pinia';
import { ref } from 'vue';
import {
  createWorkflow, listWorkflows, getWorkflow, stopWorkflow, deleteWorkflow as deleteWorkflowCmd,
  retryWorkflowChapters, listWorkflowChapters, listTransformationSourceChapters,
  listChapterWorkflowResults, promoteWorkflow, listPromotedDataAssetsForWorkflow,
  regenerateChapterPreview, commitChapterPreview, listChapterPreviews, discardChapterPreview,
} from '../ipc/commands';
import type {
  CreateWorkflowInput, WorkflowSummary, WorkflowChapterRow, SourceChapterRow,
  ChapterWorkflowResultRow, DataAsset, DeleteWorkflowResult,
  ChapterPreviewRow, CommitPreviewInput,
} from '../ipc/types';

export const useWorkflowsStore = defineStore('workflows', () => {
  const byTn = ref<Map<number, WorkflowSummary[]>>(new Map());
  const chaptersByBatch = ref<Map<number, WorkflowChapterRow[]>>(new Map());
  const sourcesByTn = ref<Map<number, SourceChapterRow[]>>(new Map());
  const resultsByTnChapter = ref<Map<string, ChapterWorkflowResultRow[]>>(new Map());
  /// preview 行按 (batchId, chapterId) 索引 —— key 用 `${batchId}:${chapterId}`(与现有 resultsByTnChapter 一致)。
  const previewsByBatchChapter = ref<Map<string, ChapterPreviewRow[]>>(new Map());
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function loadSources(tnId: number) {
    sourcesByTn.value.set(tnId, await listTransformationSourceChapters(tnId));
  }
  async function loadByTn(tnId: number) {
    byTn.value.set(tnId, await listWorkflows(tnId));
  }
  async function loadChapters(batchId: number) {
    chaptersByBatch.value.set(batchId, await listWorkflowChapters(batchId));
  }
  async function loadResultsForChapter(tnId: number, chapterId: number) {
    resultsByTnChapter.value.set(`${tnId}:${chapterId}`,
      await listChapterWorkflowResults(tnId, chapterId));
  }
  async function create(payload: CreateWorkflowInput): Promise<WorkflowSummary> {
    loading.value = true;
    try {
      const w = await createWorkflow(payload);
      const list = byTn.value.get(w.tn_id) ?? [];
      list.unshift(w);
      byTn.value.set(w.tn_id, list);
      return w;
    } finally { loading.value = false; }
  }
async function refresh(batchId: number) {
    const w = await getWorkflow(batchId);
    const list = byTn.value.get(w.tn_id);
    if (list) {
      const i = list.findIndex(x => x.id === batchId);
      if (i >= 0) list[i] = w; else list.unshift(w);
    }
  }
  async function stop(batchId: number) {
    const w = await stopWorkflow(batchId);
    await refresh(batchId);
    return w;
  }
  async function retry(batchId: number, chapterIds: number[]) {
    const w = await retryWorkflowChapters(batchId, chapterIds);
    await refresh(batchId);
    return w;
  }
  /// 删除工作流。后端 cascade 处理衍生表;store 这边从 byTn 缓存里同步移除。
  async function deleteWorkflow(batchId: number): Promise<DeleteWorkflowResult> {
    const res = await deleteWorkflowCmd(batchId);
    // 找出所属 tn(任一缓存命中即可),从 byTn 中移除。
    for (const [tnId, list] of byTn.value) {
      const i = list.findIndex((w) => w.id === batchId);
      if (i >= 0) {
        list.splice(i, 1);
        byTn.value.set(tnId, [...list]);
        break;
      }
    }
    chaptersByBatch.value.delete(batchId);
    promotedByBatch.value.delete(batchId);
    return res;
  }

  // promoted data assets 派生索引:batchId -> 列表
  const promotedByBatch = ref<Map<number, DataAsset[]>>(new Map());

  async function promote(batchId: number, title: string): Promise<DataAsset> {
    const newDa = await promoteWorkflow({ batchId, title });
    await refresh(batchId);
    const list = promotedByBatch.value.get(batchId) ?? [];
    list.unshift(newDa);
    promotedByBatch.value.set(batchId, list);
    return newDa;
  }
  async function loadPromotedByBatch(batchId: number) {
    promotedByBatch.value.set(batchId, await listPromotedDataAssetsForWorkflow(batchId));
  }

  /// 拉取某章节所有 preview 行(覆盖本地缓存)。
  async function loadPreviews(batchId: number, chapterId: number) {
    const key = `${batchId}:${chapterId}`;
    previewsByBatchChapter.value.set(key, await listChapterPreviews(batchId, chapterId));
  }

  /// 发起预览生成。返回新 preview.id;store 已乐观插入 status='generating' 的占位行,
  /// 后续通过 loadPreviews 拉真值替换。
  async function regeneratePreview(
    batchId: number, chapterId: number, customInput: string | null,
  ): Promise<number> {
    const id = await regenerateChapterPreview({
      batch_id: batchId, chapter_id: chapterId, custom_input: customInput,
    });
    await loadPreviews(batchId, chapterId);
    return id;
  }

  async function commitPreview(payload: CommitPreviewInput): Promise<WorkflowSummary> {
    const w = await commitChapterPreview(payload);
    // 提交后该章节 preview 行被删,清掉本地缓存,refresh workflow 计数
    const key = `${payload.batch_id}:${payload.chapter_id}`;
    previewsByBatchChapter.value.delete(key);
    await refresh(payload.batch_id);
    return w;
  }

  async function discardPreview(previewId: number) {
    await discardChapterPreview(previewId);
    // 不传 batchId/chapterId —— 找到包含该 preview 的 key 重新加载
    for (const [k, list] of previewsByBatchChapter.value) {
      if (list.some(p => p.id === previewId)) {
        const [bid, cid] = k.split(':').map(Number);
        await loadPreviews(bid, cid);
        return;
      }
    }
  }

  return {
    byTn, chaptersByBatch, sourcesByTn, resultsByTnChapter, promotedByBatch,
    previewsByBatchChapter,
    loading, error, loadSources, loadByTn, loadChapters, loadResultsForChapter,
    loadPromotedByBatch, create, refresh, stop, retry, promote,
    loadPreviews, regeneratePreview, commitPreview, discardPreview,
    deleteWorkflow,
  };
});
