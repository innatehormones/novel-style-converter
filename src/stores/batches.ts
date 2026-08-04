import { defineStore } from 'pinia';
import { ref } from 'vue';
import {
  countBatchesByStatus,
  getBatch,
  listBatches,
  resumeBatch,
} from '../ipc/commands';
import type { Batch, BatchStatusCount, ResumeAction } from '../ipc/types';

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

  return { byTn, loading, error, loadByTn, refresh, resume, getByTn, getCounts };
});
