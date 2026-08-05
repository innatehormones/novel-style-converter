// 临时 shim:Task 10 删除整个文件 + 替换为 `useWorkflowsStore` 之前,
// 仍以旧 API 暴露给 `CreateBatchDialog` / `Library` / `TransformationNovelDetail`。
// 数据源已切到 workflow IPC(`listWorkflows` / `getWorkflow`);旧 batch 字段
// (on_failure_policy / pending-paused 等)以类型 shim 形式留下,值是占位。
import { defineStore } from 'pinia';
import { ref } from 'vue';
import {
  listWorkflows,
  getWorkflow,
} from '../ipc/commands';
import type { WorkflowSummary } from '../ipc/types';

export type Batch = WorkflowSummary;

export type OnFailurePolicy = 'pause_and_review' | 'terminate' | 'skip_failed';

export interface BatchStatusCount {
  pending: number;
  running: number;
  paused: number;
  completed: number;
  terminated: number;
  cancelled: number;
}

export interface CreateBatchInput {
  tn_id: number;
  label: string | null;
  on_failure_policy: OnFailurePolicy;
  chapter_ids: number[];
}

export interface DispatchBatchInput {
  batch_id: number;
  overrides?: {
    prompt_id?: number | null;
    model_config_id?: number | null;
    mode?: 'compress' | 'style' | null;
    ctx_prev_original?: number | null;
    ctx_prev_transformed?: number | null;
    ctx_next_original?: number | null;
  };
}

export type ResumeAction =
  | { kind: 'retry'; chapter_id: number }
  | { kind: 'skip'; chapter_id: number }
  | { kind: 'terminate' };

export const useBatchesStore = defineStore('batches', () => {
  const byTn = ref<Map<number, Batch[]>>(new Map());
  const counts = ref<Map<number, BatchStatusCount>>(new Map());
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function loadByTn(tnId: number) {
    loading.value = true;
    error.value = null;
    try {
      const [list] = await Promise.all([
        listWorkflows(tnId),
        Promise.resolve({ pending: 0, running: 0, paused: 0, completed: 0, terminated: 0, cancelled: 0 }) as Promise<BatchStatusCount>,
      ]);
      byTn.value.set(tnId, list);
      counts.value.set(tnId, { pending: 0, running: 0, paused: 0, completed: 0, terminated: 0, cancelled: 0 });
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  async function refresh(batchId: number) {
    try {
      const b = await getWorkflow(batchId);
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

  // Task 10 删除本 store 前,createAndDispatch / resume 不再可用 —— 让
  // `Library.vue` 直接调用 `createWorkflow` IPC,绕过旧两步合一。
  async function createAndDispatch(): Promise<Batch> {
    throw new Error('Task 10:请走 createWorkflow');
  }

  async function resume(): Promise<Batch> {
    throw new Error('Task 10:请走 stopWorkflow / retryWorkflowChapters');
  }

  function getByTn(tnId: number): Batch[] {
    return byTn.value.get(tnId) ?? [];
  }

  function getCounts(tnId: number): BatchStatusCount | undefined {
    return counts.value.get(tnId);
  }

  return { byTn, loading, error, loadByTn, refresh, resume, createAndDispatch, getByTn, getCounts };
});

// 旧 free-function shim,非 Pinia 调用点(如 `Library` / Dialog)用得到。
export async function listBatches(tnId: number): Promise<Batch[]> {
  return listWorkflows(tnId);
}
export async function getBatch(batchId: number): Promise<Batch> {
  return getWorkflow(batchId);
}
export async function countBatchesByStatus(_tnId: number): Promise<BatchStatusCount> {
  return { pending: 0, running: 0, paused: 0, completed: 0, terminated: 0, cancelled: 0 };
}
export async function createBatch(_payload: CreateBatchInput): Promise<number> {
  throw new Error('Task 10:请走 createWorkflow');
}
export async function dispatchBatch(_input: DispatchBatchInput): Promise<Batch> {
  throw new Error('Task 10:请走 createWorkflow');
}
export async function updateBatch(_batchId: number, _payload: unknown): Promise<void> {
  // workflow 域无 label / policy 更新,no-op。
}
export async function listBatchChapters(_batchId: number): Promise<never[]> {
  return [];
}
export async function resumeBatch(_batchId: number, _action: ResumeAction): Promise<Batch> {
  throw new Error('Task 10:请走 stopWorkflow / retryWorkflowChapters');
}