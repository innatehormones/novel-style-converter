import { describe, it, expect, vi, beforeEach } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import { useWorkflowsStore } from '../stores/workflows';
import type { WorkflowSummary, WorkflowChapterRow, SourceChapterRow, ChapterWorkflowResultRow } from '../ipc/types';

function sampleWorkflow(overrides: Partial<WorkflowSummary> = {}): WorkflowSummary {
  return {
    id: 9,
    tn_id: 3,
    label: 'v1',
    status: 'running',
    created_at: '2026-08-05T00:00:00Z',
    started_at: '2026-08-05T00:00:00Z',
    ended_at: null,
    done_count: 0,
    failed_count: 0,
    skipped_count: 0,
    total_count: 2,
    ...overrides,
  };
}

function sampleSourceChapter(overrides: Partial<SourceChapterRow> = {}): SourceChapterRow {
  return {
    chapter_id: 10,
    idx: 1,
    title: '第1章',
    word_count: 100,
    non_empty_result_count: 0,
    ...overrides,
  };
}

function sampleWorkflowChapter(overrides: Partial<WorkflowChapterRow> = {}): WorkflowChapterRow {
  return {
    tc_id: 100,
    chapter_id: 10,
    chapter_idx: 1,
    chapter_title: '第1章',
    status: 'pending',
    error: null,
    content_preview: null,
    is_empty_slot: true,
    ...overrides,
  };
}

function sampleChapterResult(overrides: Partial<ChapterWorkflowResultRow> = {}): ChapterWorkflowResultRow {
  return {
    batch_id: 9,
    batch_label: 'v1',
    batch_status: 'running',
    batch_ended_at: null,
    content: null,
    status: 'pending',
    ...overrides,
  };
}

describe('useWorkflowsStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(invoke).mockReset();
  });

  it('createAndRun invokes create_workflow and unshifts to byTn', async () => {
    const w = sampleWorkflow({ id: 9, tn_id: 3 });
    vi.mocked(invoke).mockResolvedValueOnce(w);
    const store = useWorkflowsStore();
    const result = await store.createAndRun({
      tn_id: 3,
      label: 'v1',
      chapter_ids: [10, 11],
      prompt_id: 1,
      model_config_id: 1,
      mode: 'compress',
      ctx_prev_original: 0,
      ctx_prev_transformed: 0,
      ctx_next_original: 0,
    });
    expect(result.id).toBe(9);
    expect(invoke).toHaveBeenCalledWith('create_workflow', expect.objectContaining({ payload: expect.any(Object) }));
    expect(store.byTn.get(3)?.[0].id).toBe(9);
    expect(store.loading).toBe(false);
  });

  it('loadSources populates sourcesByTn[tid]', async () => {
    const sources = [sampleSourceChapter({ chapter_id: 10 }), sampleSourceChapter({ chapter_id: 11, idx: 2 })];
    vi.mocked(invoke).mockResolvedValueOnce(sources);
    const store = useWorkflowsStore();
    await store.loadSources(7);
    expect(invoke).toHaveBeenCalledWith('list_transformation_source_chapters', { tnId: 7 });
    expect(store.sourcesByTn.get(7)).toEqual(sources);
  });

  it('loadByTn populates byTn[tid]', async () => {
    const list = [sampleWorkflow({ id: 1 }), sampleWorkflow({ id: 2 })];
    vi.mocked(invoke).mockResolvedValueOnce(list);
    const store = useWorkflowsStore();
    await store.loadByTn(5);
    expect(invoke).toHaveBeenCalledWith('list_workflows', { tnId: 5 });
    expect(store.byTn.get(5)).toEqual(list);
  });

  it('loadChapters populates chaptersByBatch[bid]', async () => {
    const chapters = [sampleWorkflowChapter({ tc_id: 1 }), sampleWorkflowChapter({ tc_id: 2 })];
    vi.mocked(invoke).mockResolvedValueOnce(chapters);
    const store = useWorkflowsStore();
    await store.loadChapters(42);
    expect(invoke).toHaveBeenCalledWith('list_workflow_chapters', { batchId: 42 });
    expect(store.chaptersByBatch.get(42)).toEqual(chapters);
  });

  it('loadResultsForChapter populates resultsByTnChapter[`${tid}:${cid}`]', async () => {
    const results = [sampleChapterResult({ batch_id: 9 })];
    vi.mocked(invoke).mockResolvedValueOnce(results);
    const store = useWorkflowsStore();
    await store.loadResultsForChapter(7, 22);
    expect(invoke).toHaveBeenCalledWith('list_chapter_workflow_results', { tnId: 7, chapterId: 22 });
    expect(store.resultsByTnChapter.get('7:22')).toEqual(results);
  });

  it('stop invokes stop_workflow and refreshes byTn', async () => {
    const stopped = sampleWorkflow({ id: 9, tn_id: 3, status: 'stopped' });
    const refreshed = sampleWorkflow({ id: 9, tn_id: 3, status: 'stopped', ended_at: '2026-08-05T00:01:00Z' });
    vi.mocked(invoke)
      .mockResolvedValueOnce(stopped)   // stop_workflow 返回
      .mockResolvedValueOnce(refreshed); // get_workflow 返回
    const store = useWorkflowsStore();
    // 先 put 一个旧记录进 byTn,以便 refresh 能找到它并替换
    store.byTn.set(3, [sampleWorkflow({ id: 9, tn_id: 3, status: 'running' })]);
    const result = await store.stop(9);
    expect(result.status).toBe('stopped');
    expect(invoke).toHaveBeenCalledWith('stop_workflow', { batchId: 9 });
    expect(invoke).toHaveBeenCalledWith('get_workflow', { batchId: 9 });
    expect(store.byTn.get(3)?.[0].status).toBe('stopped');
    expect(store.byTn.get(3)?.[0].ended_at).toBe('2026-08-05T00:01:00Z');
  });

  it('retry invokes retry_workflow_chapters and refreshes byTn', async () => {
    const retried = sampleWorkflow({ id: 9, tn_id: 3, status: 'running' });
    const refreshed = sampleWorkflow({ id: 9, tn_id: 3, status: 'running', done_count: 1 });
    vi.mocked(invoke)
      .mockResolvedValueOnce(retried)
      .mockResolvedValueOnce(refreshed);
    const store = useWorkflowsStore();
    store.byTn.set(3, [sampleWorkflow({ id: 9, tn_id: 3, status: 'stopped' })]);
    await store.retry(9, [10, 11]);
    expect(invoke).toHaveBeenCalledWith('retry_workflow_chapters', { batchId: 9, chapterIds: [10, 11] });
    expect(invoke).toHaveBeenCalledWith('get_workflow', { batchId: 9 });
    expect(store.byTn.get(3)?.[0].status).toBe('running');
    expect(store.byTn.get(3)?.[0].done_count).toBe(1);
  });
});
