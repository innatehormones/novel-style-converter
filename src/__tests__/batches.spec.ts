import { describe, expect, it, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import {
  listBatches,
  getBatch,
  createBatch,
  dispatchBatch,
  updateBatch,
  listBatchChapters,
  countBatchesByStatus,
} from '../ipc/commands';

describe('batches IPC wrappers', () => {
  beforeEach(() => { (invoke as ReturnType<typeof vi.fn>).mockReset(); });

  it('listBatches invokes "list_batches" with { tnId } camelCase', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce([]);
    await listBatches(7);
    expect(invoke).toHaveBeenCalledWith('list_batches', { tnId: 7 });
  });

  it('getBatch invokes "get_batch" with { batchId }', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ id: 3, tn_id: 1 });
    await getBatch(3);
    expect(invoke).toHaveBeenCalledWith('get_batch', { batchId: 3 });
  });

  it('createBatch passes inner payload as snake_case', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce(42);
    await createBatch({
      tn_id: 1,
      label: null,
      on_failure_policy: 'pause_and_review',
      chapter_ids: [10, 11],
    });
    expect(invoke).toHaveBeenCalledWith('create_batch', {
      payload: {
        tn_id: 1,
        label: null,
        on_failure_policy: 'pause_and_review',
        chapter_ids: [10, 11],
      },
    });
  });

  it('updateBatch passes { batchId, payload }', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce(undefined);
    await updateBatch(3, { label: 'new' });
    expect(invoke).toHaveBeenCalledWith('update_batch', {
      batchId: 3,
      payload: { label: 'new' },
    });
  });

  it('listBatchChapters invokes with batchId', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce([]);
    await listBatchChapters(9);
    expect(invoke).toHaveBeenCalledWith('list_batch_chapters', { batchId: 9 });
  });

  it('countBatchesByStatus invokes with tnId', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      pending: 0, running: 0, paused: 0, completed: 0, terminated: 0, cancelled: 0,
    });
    await countBatchesByStatus(2);
    expect(invoke).toHaveBeenCalledWith('count_batches_by_status', { tnId: 2 });
  });

  it('dispatchBatch passes { batchId, overrides } camelCase + default {}', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ id: 5 });
    await dispatchBatch({ batch_id: 5 });
    expect(invoke).toHaveBeenCalledWith('dispatch_batch', { batchId: 5, overrides: {} });

    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ id: 6 });
    await dispatchBatch({
      batch_id: 6,
      overrides: { prompt_id: 2, ctx_prev_original: 1 },
    });
    expect(invoke).toHaveBeenCalledWith('dispatch_batch', {
      batchId: 6,
      overrides: { prompt_id: 2, ctx_prev_original: 1 },
    });
  });
});
