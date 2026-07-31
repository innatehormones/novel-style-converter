import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { ModelConfig, ModelConfigInput } from '../ipc/types';
import {
  deleteModel as ipcDeleteModel,
  listModels as ipcListModels,
  testModel as ipcTestModel,
  upsertModel as ipcUpsertModel,
} from '../ipc/commands';

export const useModelsStore = defineStore('models', () => {
  const models = ref<ModelConfig[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const testing = ref(false);

  async function load() {
    loading.value = true;
    error.value = null;
    try {
      models.value = await ipcListModels();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  async function save(input: ModelConfigInput): Promise<number> {
    const id = await ipcUpsertModel(input);
    await load();
    return id;
  }

  async function remove(id: number): Promise<void> {
    await ipcDeleteModel(id);
    models.value = models.value.filter((m) => m.id !== id);
  }

  async function test(input: ModelConfigInput): Promise<string> {
    testing.value = true;
    error.value = null;
    try {
      return await ipcTestModel(input);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      error.value = msg;
      throw new Error(msg);
    } finally {
      testing.value = false;
    }
  }

  return { models, loading, error, testing, load, save, remove, test };
});
