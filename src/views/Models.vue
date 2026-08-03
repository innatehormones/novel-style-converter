<template>
  <section>
    <header class="header">
      <h2>模型</h2>
      <div class="actions">
        <Button kind="primary" @click="openCreate">新增模型</Button>
      </div>
    </header>

    <div v-if="store.error" class="alert">{{ store.error }}</div>

    <div v-if="!store.loading && store.models.length === 0" class="empty">
      暂无模型，请先新增一个模型配置。
    </div>

    <Table
      v-else
      :columns="columns"
      :data="store.models"
      empty-text="暂无模型"
      :row-key="(row) => row.id"
    >
      <template #cell-id="{ row }">{{ row.id }}</template>
      <template #cell-name="{ row }">{{ row.name }}</template>
      <template #cell-model="{ row }">{{ row.model }}</template>
      <template #cell-base_url="{ row }">{{ row.base_url }}</template>
      <template #cell-actions="{ row }">
        <Button size="small" @click="openEdit(row)">编辑</Button>
        <Button size="small" kind="danger" @click="onDelete(row.id)">
          删除
        </Button>
      </template>
    </Table>

    <ModelDialog
      v-model:open="dialogOpen"
      :initial="dialogInitial"
      @submit="onSubmit"
    />

    <ConfirmDialog
      v-model:open="deleteConfirmOpen"
      title="删除模型"
      message="确认删除这个模型?"
      kind="danger"
      confirm-text="删除"
      @confirm="doDelete"
    />
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue';
import Button from '../components/ui/Button.vue';
import Table from '../components/ui/Table.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import ModelDialog from '../components/ModelDialog.vue';
import { useModelsStore } from '../stores/models';
import type { ModelConfigInput } from '../ipc/types';

const store = useModelsStore();
const dialogOpen = ref(false);
const dialogInitial = ref<ModelConfigInput | null>(null);
const deleteConfirmOpen = ref(false);
const deleteTargetId = ref<number | null>(null);

const columns = [
  { key: 'id', title: 'id', width: '60px' },
  { key: 'name', title: '名称', width: '160px' },
  { key: 'model', title: '模型', width: '160px' },
  { key: 'base_url', title: 'Base URL' },
  { key: 'actions', title: '操作', width: '180px' },
];

onMounted(() => store.load());

function openCreate() {
  dialogInitial.value = null;
  dialogOpen.value = true;
}

function openEdit(row: ModelConfigInput) {
  dialogInitial.value = { ...row };
  dialogOpen.value = true;
}

async function onSubmit(input: ModelConfigInput) {
  await store.save(input);
}

async function onDelete(id: number) {
  deleteTargetId.value = id;
  deleteConfirmOpen.value = true;
}

async function doDelete() {
  const id = deleteTargetId.value;
  if (id == null) return;
  await store.remove(id);
}
</script>

<style scoped>
.header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  margin-bottom: 24px;
  padding-bottom: 16px;
  border-bottom: 1px solid var(--border-rouge);
}
.header h2 {
  margin: 0;
  font-family: var(--font-serif);
  font-size: 28px;
  letter-spacing: -0.01em;
}
.actions { display: flex; gap: 12px; align-items: center; }
.alert {
  padding: 12px 16px;
  background: var(--danger-bg);
  color: var(--danger);
  border-radius: var(--radius-pin);
  margin-bottom: 16px;
  border: 1px solid var(--danger-border);
  font-family: var(--font-serif);
}
.empty {
  text-align: center;
  padding: 56px 0;
  color: var(--text-muted);
  border: 1px dashed var(--border-rouge);
  border-radius: var(--radius-card);
  background: var(--color-sheet);
  font-family: var(--font-serif);
  font-size: 15px;
  font-style: italic;
}
</style>
