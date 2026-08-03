<template>
  <section>
    <header class="header">
      <h2>提示词</h2>
      <div class="actions">
        <Button kind="primary" @click="openCreate">新建 prompt</Button>
      </div>
    </header>

    <div v-if="store.error" class="alert">{{ store.error }}</div>

    <div v-if="!store.loading && store.prompts.length === 0" class="empty">
      还没有提示词,点击右上"新建 prompt"创建一条。
    </div>
    <Table
      v-else
      :columns="columns"
      :data="store.prompts"
      :row-key="(row: Prompt) => row.id"
    >
      <template #cell-name="{ row }">{{ row.name }}</template>
      <template #cell-kind="{ row }">
        <span class="kind-tag" :class="`kind-${row.kind}`">
          {{ row.kind === 'compress' ? '压缩' : '文风' }}
        </span>
      </template>
      <template #cell-builtin="{ row }">
        <Tag v-if="row.is_builtin" kind="info">内置</Tag>
        <span v-else class="muted">用户</span>
      </template>
      <template #cell-actions="{ row }">
        <Button v-if="row.is_builtin" size="small" @click="openView(row)">查看</Button>
        <Button v-else size="small" @click="openEdit(row)">编辑</Button>
        <Button size="small" @click="openCopy(row)">复制</Button>
        <Button
          size="small"
          kind="danger"
          :disabled="row.is_builtin"
          @click="requestDelete(row)"
        >删除</Button>
      </template>
    </Table>

    <PromptEditDialog
      v-model:open="dialogOpen"
      :mode="dialogMode"
      :initial="dialogInitial"
      @saved="onSaved"
    />

    <PromptViewDialog
      v-if="viewTarget"
      v-model:open="viewOpen"
      :initial="viewTarget"
    />

    <Dialog
      v-model:open="confirmOpen"
      title="删除提示词"
      :width="420"
    >
      <div v-if="pendingDelete">
        <p>确认删除提示词"<strong>{{ pendingDelete.name }}</strong>"?</p>
        <p v-if="pendingDelete.usage > 0" class="warn">
          该 prompt 当前被 {{ pendingDelete.usage }} 个转换结果引用,删除后这些结果仍保留历史引用,但新建转换时无法再选用。
        </p>
      </div>
      <template #footer>
        <Button @click="confirmOpen = false">取消</Button>
        <Button kind="danger" @click="confirmDelete">确认删除</Button>
      </template>
    </Dialog>

    <AlertDialog
      v-model:open="alertOpen"
      title="提示"
      :message="alertMessage"
    />
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue';
import Button from '../components/ui/Button.vue';
import Table from '../components/ui/Table.vue';
import Tag from '../components/ui/Tag.vue';
import Dialog from '../components/ui/Dialog.vue';
import AlertDialog from '../components/ui/AlertDialog.vue';
import PromptEditDialog from '../components/PromptEditDialog.vue';
import PromptViewDialog from '../components/PromptViewDialog.vue';
import { usePromptsStore } from '../stores/prompts';
import type { Prompt } from '../ipc/types';

const store = usePromptsStore();

type DialogMode = 'create' | 'edit' | 'copy-from-builtin';

const columns = [
  { key: 'name', title: '名称', width: '240px' },
  { key: 'kind', title: '类型', width: '100px' },
  { key: 'builtin', title: '来源', width: '120px' },
  { key: 'actions', title: '操作', width: '280px', type: 'actions' as const },
];

const dialogOpen = ref(false);
const dialogMode = ref<DialogMode>('create');
const dialogInitial = ref<Prompt | undefined>(undefined);

const viewOpen = ref(false);
const viewTarget = ref<Prompt | null>(null);

interface PendingDelete {
  id: number;
  name: string;
  usage: number;
}
const confirmOpen = ref(false);
const pendingDelete = ref<PendingDelete | null>(null);
const alertOpen = ref(false);
const alertMessage = ref('');

onMounted(() => void store.load());

function openCreate() {
  dialogMode.value = 'create';
  dialogInitial.value = undefined;
  dialogOpen.value = true;
}

function openEdit(row: Prompt) {
  dialogMode.value = 'edit';
  dialogInitial.value = row;
  dialogOpen.value = true;
}

function openView(row: Prompt) {
  viewTarget.value = row;
  viewOpen.value = true;
}

function openCopy(row: Prompt) {
  dialogMode.value = 'copy-from-builtin';
  dialogInitial.value = row;
  dialogOpen.value = true;
}

function onSaved() {
}

let deleteRequestId = 0;

async function requestDelete(row: Prompt) {
  const requestId = ++deleteRequestId;
  let usage = 0;
  try {
    usage = await store.countUsage(row.id);
  } catch {
    usage = 0;
  }
  if (requestId !== deleteRequestId) return;
  pendingDelete.value = { id: row.id, name: row.name, usage };
  confirmOpen.value = true;
}

async function confirmDelete() {
  const pending = pendingDelete.value;
  if (!pending) return;
  confirmOpen.value = false;
  pendingDelete.value = null;
  try {
    await store.remove(pending.id);
  } catch (e: unknown) {
    alertMessage.value = e instanceof Error ? e.message : String(e);
    alertOpen.value = true;
  }
}
</script>

<style scoped>
.header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  margin-bottom: 16px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--border-color);
}
.header h2 {
  margin: 0;
  font-size: 24px;
  font-weight: var(--font-weight-medium);
}
.actions { display: flex; gap: 12px; align-items: center; }
.alert {
  padding: 12px 16px;
  background: var(--bg-hover);
  color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin);
  margin-bottom: 16px;
}
.empty {
  text-align: center;
  padding: 56px 0;
  color: var(--text-secondary);
  border: 1px dashed var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
}
.kind-tag {
  display: inline-block;
  padding: 2px 10px;
  border-radius: var(--radius-pin);
  font-size: 12px;
}
.kind-compress {
  background: var(--color-paper-mist);
  color: var(--text-primary);
}
.kind-style {
  background: var(--color-cinnabar-light);
  color: var(--color-cinnabar-deep);
}
.muted {
  color: var(--text-secondary);
  font-size: 13px;
}
.warn {
  margin-top: 12px;
  padding: 8px 12px;
  background: #fff8e1;
  color: #8a6d3b;
  border-radius: var(--radius-pin);
  font-size: 12px;
}
</style>
