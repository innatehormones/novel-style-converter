<template>
  <section>
    <PageHeader title="模型" subtitle="OpenAI 兼容 API 配置，可在此新增 / 编辑 / 软删">
      <template #actions>
        <Button kind="primary" @click="openCreate">新增模型</Button>
      </template>
    </PageHeader>

    <div class="toolbar">
      <label class="toggle">
        <input
          type="checkbox"
          :checked="store.includeArchived"
          @change="onToggleArchived(($event.target as HTMLInputElement).checked)"
        />
        <span>显示已归档</span>
      </label>
    </div>

    <div v-if="store.error" class="alert">{{ store.error }}</div>

    <div v-if="!store.loading && !store.hasAnyModel" class="empty">
      <p class="empty-title">尚未配置任何模型</p>
      <p class="empty-hint">
        模型是 LLM 转换的前提。点"新增模型"填一份 OpenAI 兼容 endpoint 的 base_url / api_key / model 即可。
      </p>
      <p class="empty-hint subtle">
        注：本应用不会从环境变量自动注入默认模型；一切模型配置都需要在此处显式新增。
      </p>
    </div>

    <Table
      v-else
      :columns="columns"
      :data="store.models"
      empty-text="暂无模型"
      :row-key="(row) => row.id"
    >
      <template #cell-id="{ row }">
        {{ row.id }}
        <Tag v-if="row.archived === 1" kind="info" class="archived-tag">已归档</Tag>
      </template>
      <template #cell-name="{ row }">
        <span :class="{ archived: row.archived === 1 }">{{ row.name }}</span>
      </template>
      <template #cell-model="{ row }">{{ row.model }}</template>
      <template #cell-base_url="{ row }">{{ row.base_url }}</template>
      <template #cell-concurrency="{ row }">{{ row.concurrency }}</template>
      <template #cell-actions="{ row }">
        <template v-if="row.archived === 1">
          <Button size="small" @click="onRestore(row.id)">恢复</Button>
        </template>
        <template v-else>
          <Button size="small" @click="openEdit(row)">编辑</Button>
          <Button size="small" kind="danger" @click="onDelete(row.id)">
            删除
          </Button>
        </template>
      </template>
    </Table>

    <ModelDialog
      v-model:open="dialogOpen"
      :initial="dialogInitial"
      @submit="onSubmit"
    />

    <ConfirmDialog
      v-model:open="deleteConfirmOpen"
      title="归档模型"
      message="归档后会清空 API key 并隐藏该行，历史转换结果仍可显示来源 model 名 / 端点 / 并发配置。确认归档？"
      kind="danger"
      confirm-text="归档"
      @confirm="doDelete"
    />
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue';
import Button from '../components/ui/Button.vue';
import PageHeader from '../components/ui/PageHeader.vue';
import Table from '../components/ui/Table.vue';
import Tag from '../components/ui/Tag.vue';
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
  { key: 'id', title: 'id', width: '90px' },
  { key: 'name', title: '名称', width: '160px' },
  { key: 'model', title: '模型', width: '160px' },
  { key: 'base_url', title: 'Base URL' },
  { key: 'concurrency', title: '并发', width: '70px' },
  { key: 'actions', title: '操作', width: '180px', type: 'actions' as const },
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

function onDelete(id: number) {
  deleteTargetId.value = id;
  deleteConfirmOpen.value = true;
}

async function doDelete() {
  const id = deleteTargetId.value;
  if (id == null) return;
  await store.remove(id);
}

async function onRestore(id: number) {
  await store.restore(id);
}

async function onToggleArchived(v: boolean) {
  await store.setIncludeArchived(v);
}
</script>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 12px;
}
.toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text-secondary);
  cursor: pointer;
  user-select: none;
}
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
  padding: 48px 24px;
  color: var(--text-muted);
  border: 1px dashed var(--border-rouge);
  border-radius: var(--radius-card);
  background: var(--color-sheet);
  font-family: var(--font-serif);
}
.empty-title {
  font-size: 18px;
  color: var(--text-primary);
  margin: 0 0 12px;
}
.empty-hint {
  font-size: 14px;
  margin: 0 0 6px;
  line-height: 1.6;
}
.empty-hint.subtle {
  color: var(--text-muted);
  font-style: italic;
  font-size: 12px;
}
.archived {
  color: var(--text-muted);
  text-decoration: line-through;
}
.archived-tag {
  margin-left: 6px;
}
</style>

