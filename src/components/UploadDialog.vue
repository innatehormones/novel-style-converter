<template>
  <Dialog v-model:open="open" title="上传 .txt 文件" :width="480">
    <div class="row">
      <label>文本文件 *</label>
      <Button kind="primary" :disabled="picking" @click="onPick">
        {{ picking ? '选择中...' : (filePath ? '重新选择' : '选择文件') }}
      </Button>
    </div>
    <div v-if="fileInfo" class="file-info">
      {{ fileInfo.name }} · {{ fileInfo.path }}
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <template #footer>
      <Button :disabled="submitting" @click="open = false">取消</Button>
      <Button kind="primary" :loading="submitting" :disabled="!canSubmit || submitting" @click="onSubmit">上传</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';

const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{ submit: [{ filePath: string; filename: string }] }>();

const filePath = ref('');
const filename = ref('');
const error = ref<string | null>(null);
const submitting = ref(false);
const picking = ref(false);

const fileInfo = computed(() =>
  filePath.value ? { name: filename.value, path: filePath.value } : null,
);

const canSubmit = computed(() => filePath.value !== '');

watch(open, (v) => {
  if (v) {
    filePath.value = '';
    filename.value = '';
    error.value = null;
    submitting.value = false;
    picking.value = false;
  }
});

async function onPick() {
  error.value = null;
  picking.value = true;
  try {
    const selected = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: 'Text', extensions: ['txt'] }],
    });
    if (typeof selected === 'string') {
      filePath.value = selected;
      // use last path segment as default display name
      const segs = selected.split(/[\\\\/]/);
      filename.value = segs[segs.length - 1] || 'uploaded.txt';
    }
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    picking.value = false;
  }
}

function onSubmit() {
  if (!canSubmit.value) return;
  error.value = null;
  submitting.value = true;
  try {
    emit('submit', { filePath: filePath.value, filename: filename.value });
    open.value = false;
  } finally {
    submitting.value = false;
  }
}
</script>

<style scoped>
.row {
  display: flex;
  align-items: center;
  margin-bottom: 12px;
  gap: 12px;
}
.row label {
  width: 90px;
  font-size: 14px;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.file-info {
  font-size: 12px;
  color: var(--text-muted);
  margin-bottom: 12px;
  word-break: break-all;
}
.error {
  color: var(--danger);
  font-size: 12px;
  margin-bottom: 8px;
}
</style>
