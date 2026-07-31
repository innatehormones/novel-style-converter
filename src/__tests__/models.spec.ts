import { setActivePinia, createPinia } from 'pinia';
import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { useModelsStore } from '../stores/models';
import type { ModelConfig } from '../ipc/types';

describe('models store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(invoke).mockReset();
  });

  it('load calls list_models and stores result', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      {
        id: 1,
        name: 'gpt-4',
        base_url: 'https://api.openai.com/v1',
        api_key: 'sk-test',
        model: 'gpt-4',
        max_tokens: 2048,
        temperature: 0.7,
        concurrency: 1,
      },
    ] as ModelConfig[]);

    const store = useModelsStore();
    await store.load();

    expect(invoke).toHaveBeenCalledWith('list_models');
    expect(store.models).toHaveLength(1);
    expect(store.models[0].name).toBe('gpt-4');
    expect(store.loading).toBe(false);
  });

  it('save posts upsert_model with payload', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(7);

    const store = useModelsStore();
    const id = await store.save({
      id: 0,
      name: 'new',
      base_url: 'https://x',
      api_key: 'k',
      model: 'm',
      max_tokens: null,
      temperature: null,
      concurrency: 1,
    });

    expect(invoke).toHaveBeenCalledWith('upsert_model', {
      payload: {
        id: 0,
        name: 'new',
        base_url: 'https://x',
        api_key: 'k',
        model: 'm',
        max_tokens: null,
        temperature: null,
        concurrency: 1,
      },
    });
    expect(id).toBe(7);
  });

  it('remove deletes by id and filters list', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      { id: 1, name: 'A', base_url: '', api_key: '', model: '', max_tokens: null, temperature: null, concurrency: 1 },
      { id: 2, name: 'B', base_url: '', api_key: '', model: '', max_tokens: null, temperature: null, concurrency: 1 },
    ] as ModelConfig[]);

    const store = useModelsStore();
    await store.load();

    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await store.remove(1);

    expect(invoke).toHaveBeenCalledWith('delete_model', { id: 1 });
    expect(store.models.map((m) => m.id)).toEqual([2]);
  });

  it('test returns content from invoke', async () => {
    vi.mocked(invoke).mockResolvedValueOnce('pong');

    const store = useModelsStore();
    const result = await store.test({
      id: 0,
      name: '',
      base_url: 'https://x',
      api_key: 'k',
      model: 'm',
      max_tokens: null,
      temperature: null,
      concurrency: 1,
    });

    expect(invoke).toHaveBeenCalledWith('test_model', {
      payload: {
        id: 0,
        name: '',
        base_url: 'https://x',
        api_key: 'k',
        model: 'm',
        max_tokens: null,
        temperature: null,
        concurrency: 1,
      },
    });
    expect(result).toBe('pong');
  });

  it('captures error string', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error('http 401'));

    const store = useModelsStore();
    await store.load();

    expect(store.error).toBe('http 401');
  });
});
