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

import { setActivePinia, createPinia } from 'pinia';
import { usePromptsStore } from '../stores/prompts';

describe('prompts store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(invoke).mockReset();
  });

  it('load calls list_prompts and stores result', async () => {
    const data = [
      { id: 1, name: 'compress_default', kind: 'compress', template: '...', is_builtin: true },
      { id: 2, name: 'style_default', kind: 'style', template: '...', is_builtin: true },
      { id: 3, name: 'user', kind: 'compress', template: '...', is_builtin: false },
    ] as Prompt[];
    vi.mocked(invoke).mockResolvedValueOnce(data);

    const store = usePromptsStore();
    await store.load();

    expect(invoke).toHaveBeenCalledWith('list_prompts');
    expect(store.prompts).toHaveLength(3);
    expect(store.loading).toBe(false);
  });

  it('upsert (id=0, create) invokes upsert_prompt then reloads', async () => {
    const store = usePromptsStore();
    vi.mocked(invoke).mockResolvedValueOnce(11); // upsert
    vi.mocked(invoke).mockResolvedValueOnce([]); // load
    await store.upsert({ id: 0, name: 'new', kind: 'compress', template: 't' });

    expect(invoke).toHaveBeenNthCalledWith(1, 'upsert_prompt', {
      payload: { id: 0, name: 'new', kind: 'compress', template: 't' },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'list_prompts');
  });

  it('upsert (id>0, update) invokes upsert_prompt then reloads', async () => {
    const store = usePromptsStore();
    vi.mocked(invoke).mockResolvedValueOnce(11); // upsert
    vi.mocked(invoke).mockResolvedValueOnce([]); // load
    await store.upsert({ id: 7, name: 'edit', kind: 'style', template: 't' });

    expect(invoke).toHaveBeenNthCalledWith(1, 'upsert_prompt', {
      payload: { id: 7, name: 'edit', kind: 'style', template: 't' },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'list_prompts');
  });

  it('remove invokes delete_prompt then reloads', async () => {
    const store = usePromptsStore();
    vi.mocked(invoke).mockResolvedValueOnce(undefined); // delete
    vi.mocked(invoke).mockResolvedValueOnce([]); // load
    await store.remove(7);

    expect(invoke).toHaveBeenNthCalledWith(1, 'delete_prompt', { id: 7 });
    expect(invoke).toHaveBeenNthCalledWith(2, 'list_prompts');
  });

  it('countUsage returns number from invoke', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(5);
    const store = usePromptsStore();
    const n = await store.countUsage(7);
    expect(invoke).toHaveBeenCalledWith('count_transformation_chapters_by_prompt', { promptId: 7 });
    expect(n).toBe(5);
  });

  it('captures error string on load failure', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error('boom'));
    const store = usePromptsStore();
    await store.load();
    expect(store.error).toBe('boom');
  });
});
