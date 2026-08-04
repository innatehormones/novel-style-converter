import { defineStore } from 'pinia';
import { ref } from 'vue';
import {
  countBatchesByStatus,
  createBatch,
  dispatchBatch,
  getBatch,
  listBatches,
  resumeBatch,
} from '../ipc/commands';
import type {
  Batch,
  BatchStatusCount,
  CreateBatchInput,
  DispatchBatchInput,
  ResumeAction,
} from '../ipc/types';

export const useBatchesStore = defineStore('batches', () => {
  const byTn = ref<Map<number, Batch[]>>(new Map());
  const counts = ref<Map<number, BatchStatusCount>>(new Map());
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function loadByTn(tnId: number) {
    loading.value = true;
    error.value = null;
    try {
      const [batches, c] = await Promise.all([listBatches(tnId), countBatchesByStatus(tnId)]);
      byTn.value.set(tnId, batches);
      counts.value.set(tnId, c);
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  async function refresh(batchId: number) {
    try {
      const b = await getBatch(batchId);
      const list = byTn.value.get(b.tn_id);
      if (list) {
        const i = list.findIndex((x) => x.id === batchId);
        if (i >= 0) list[i] = b;
        else list.unshift(b);
      }
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    }
  }

  /// 创建 batch + 派发（落 tc + 入队）。两步合一是因为后端 `create_batch`
  /// 不会自动派发 — 必须显式调 `dispatch_batch`。
  async function createAndDispatch(payload: CreateBatchInput, dispatch: DispatchBatchInput['overrides']) {
    loading.value = true;
    error.value = null;
    try {
      const batchId = await createBatch(payload);
      const batch = await dispatchBatch({ batch_id: batchId, overrides: dispatch });
      const list = byTn.value.get(batch.tn_id) ?? [];
      list.unshift(batch);
      byTn.value.set(batch.tn_id, list);
      return batch;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      loading.value = false;
    }
  }

  async function resume(batchId: number, action: ResumeAction) {
    loading.value = true;
    error.value = null;
    try {
      const batch = await resumeBatch(batchId, action);
      const list = byTn.value.get(batch.tn_id);
      if (list) {
        const i = list.findIndex((b) => b.id === batch.id);
        if (i >= 0) list[i] = batch;
      }
      return batch;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      loading.value = false;
    }
  }

  function getByTn(tnId: number): Batch[] {
    return byTn.value.get(tnId) ?? [];
  }

  function getCounts(tnId: number): BatchStatusCount | undefined {
    return counts.value.get(tnId);
  }

  return { byTn, loading, error, loadByTn, refresh, resume, createAndDispatch, getByTn, getCounts };
});
