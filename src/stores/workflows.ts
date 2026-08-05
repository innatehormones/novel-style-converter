import { defineStore } from 'pinia';
import { ref } from 'vue';
import {
  createWorkflow, listWorkflows, getWorkflow, stopWorkflow,
  retryWorkflowChapters, listWorkflowChapters, listTransformationSourceChapters,
  listChapterWorkflowResults,
} from '../ipc/commands';
import type {
  CreateWorkflowInput, WorkflowSummary, WorkflowChapterRow, SourceChapterRow,
  ChapterWorkflowResultRow,
} from '../ipc/types';

export const useWorkflowsStore = defineStore('workflows', () => {
  const byTn = ref<Map<number, WorkflowSummary[]>>(new Map());
  const chaptersByBatch = ref<Map<number, WorkflowChapterRow[]>>(new Map());
  const sourcesByTn = ref<Map<number, SourceChapterRow[]>>(new Map());
  const resultsByTnChapter = ref<Map<string, ChapterWorkflowResultRow[]>>(new Map());
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
  async function createAndRun(payload: CreateWorkflowInput): Promise<WorkflowSummary> {
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
  return {
    byTn, chaptersByBatch, sourcesByTn, resultsByTnChapter,
    loading, error, loadSources, loadByTn, loadChapters, loadResultsForChapter,
    createAndRun, refresh, stop, retry,
  };
});
