import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import {
  listPrompts,
  getPrompt,
  upsertPrompt,
  deletePrompt,
  countPromptUsage,
} from '../ipc/commands';
import type { Prompt, PromptInput } from '../ipc/types';

describe('prompts IPC wrappers', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('listPrompts calls list_prompts', async () => {
    const payload = [
      { id: 1, name: 'compress_default', kind: 'compress', template: '...', is_builtin: true },
    ] as Prompt[];
    vi.mocked(invoke).mockResolvedValueOnce(payload);

    const result = await listPrompts();

    expect(invoke).toHaveBeenCalledWith('list_prompts');
    expect(result).toEqual(payload);
  });

  it('getPrompt calls get_prompt with id', async () => {
    const p = { id: 7, name: 'x', kind: 'style', template: '...', is_builtin: false } as Prompt;
    vi.mocked(invoke).mockResolvedValueOnce(p);

    const result = await getPrompt(7);

    expect(invoke).toHaveBeenCalledWith('get_prompt', { id: 7 });
    expect(result).toBe(p);
  });

  it('upsertPrompt calls upsert_prompt with payload wrapper', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(11);

    const input: PromptInput = {
      id: 0,
      name: 'new',
      kind: 'compress',
      template: 'hello',
    };
    const id = await upsertPrompt(input);

    expect(invoke).toHaveBeenCalledWith('upsert_prompt', {
      payload: { id: 0, name: 'new', kind: 'compress', template: 'hello' },
    });
    expect(id).toBe(11);
  });

  it('deletePrompt calls delete_prompt with id', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await deletePrompt(7);

    expect(invoke).toHaveBeenCalledWith('delete_prompt', { id: 7 });
  });

  it('countPromptUsage calls count_transformation_chapters_by_prompt with promptId', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(3);

    const n = await countPromptUsage(7);

    expect(invoke).toHaveBeenCalledWith('count_transformation_chapters_by_prompt', { promptId: 7 });
    expect(n).toBe(3);
  });
});
