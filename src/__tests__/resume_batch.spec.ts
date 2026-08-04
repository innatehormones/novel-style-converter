import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { resumeBatch } from '../ipc/commands';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('resumeBatch IPC wrapper', () => {
  beforeEach(() => vi.clearAllMocks());

  it('retry 把 chapter_id 嵌进 snake_case payload', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ id: 1, status: 'running' });
    await resumeBatch(1, { kind: 'retry', chapter_id: 7 });
    expect(invoke).toHaveBeenCalledWith('resume_batch', {
      batchId: 1,
      action: { kind: 'retry', chapter_id: 7 },
    });
  });

  it('skip 同样带 chapter_id', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ id: 1, status: 'running' });
    await resumeBatch(1, { kind: 'skip', chapter_id: 9 });
    expect(invoke).toHaveBeenCalledWith('resume_batch', {
      batchId: 1,
      action: { kind: 'skip', chapter_id: 9 },
    });
  });

  it('terminate 不带 chapter_id', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ id: 1, status: 'terminated' });
    await resumeBatch(1, { kind: 'terminate' });
    expect(invoke).toHaveBeenCalledWith('resume_batch', {
      batchId: 1,
      action: { kind: 'terminate' },
    });
  });
});