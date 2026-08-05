import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import {
  createWorkflow,
  listWorkflows,
  getWorkflow,
  listWorkflowChapters,
  stopWorkflow,
  retryWorkflowChapters,
  listTransformationSourceChapters,
  listChapterWorkflowResults,
} from '../ipc/commands';

describe('workflows IPC wrappers', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it('createWorkflow sends create_workflow + snake_case payload', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ id: 1 });
    await createWorkflow({
      tn_id: 1,
      label: 'v1',
      chapter_ids: [1, 2],
      prompt_id: 1,
      model_config_id: 1,
      mode: 'compress',
      ctx_prev_original: 0,
      ctx_prev_transformed: 0,
      ctx_next_original: 0,
    });
    expect(invoke).toHaveBeenCalledWith('create_workflow', {
      payload: {
        tn_id: 1,
        label: 'v1',
        chapter_ids: [1, 2],
        prompt_id: 1,
        model_config_id: 1,
        mode: 'compress',
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
      },
    });
  });

  it('listWorkflows invokes list_workflows with tnId', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listWorkflows(7);
    expect(invoke).toHaveBeenCalledWith('list_workflows', { tnId: 7 });
  });

  it('getWorkflow invokes get_workflow with batchId', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ id: 3 });
    await getWorkflow(3);
    expect(invoke).toHaveBeenCalledWith('get_workflow', { batchId: 3 });
  });

  it('listWorkflowChapters invokes list_workflow_chapters with batchId', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listWorkflowChapters(9);
    expect(invoke).toHaveBeenCalledWith('list_workflow_chapters', { batchId: 9 });
  });

  it('stopWorkflow invokes stop_workflow with batchId', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ id: 2 });
    await stopWorkflow(2);
    expect(invoke).toHaveBeenCalledWith('stop_workflow', { batchId: 2 });
  });

  it('retryWorkflowChapters sends { batchId, chapterIds }', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ id: 3 });
    await retryWorkflowChapters(3, [5, 6]);
    expect(invoke).toHaveBeenCalledWith('retry_workflow_chapters', {
      batchId: 3,
      chapterIds: [5, 6],
    });
  });

  it('listTransformationSourceChapters invokes with tnId', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listTransformationSourceChapters(11);
    expect(invoke).toHaveBeenCalledWith('list_transformation_source_chapters', { tnId: 11 });
  });

  it('listChapterWorkflowResults sends { tnId, chapterId }', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listChapterWorkflowResults(11, 22);
    expect(invoke).toHaveBeenCalledWith('list_chapter_workflow_results', { tnId: 11, chapterId: 22 });
  });
});