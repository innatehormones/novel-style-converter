<template>
  <Dialog v-model:open="open" title="上传 .txt 文件" :width="480">
    <div class="row">
      <label>文本文件 *</label>
      <input ref="fileInput" type="file" accept=".txt" @change="onFile" />
    </div>
    <div v-if="fileInfo" class="file-info">
      {{ fileInfo.name }} · {{ (fileInfo.size / 1024).toFixed(1) }} KB
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
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';

const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{ submit: [{ filename: string; bytes: number[] }] }>();

const filename = ref('');
const fileSize = ref(0);
const bytes = ref<number[] | null>(null);
const error = ref<string | null>(null);
const submitting = ref(false);
const fileInput = ref<HTMLInputElement | null>(null);

const fileInfo = computed(() =>
  filename.value ? { name: filename.value, size: fileSize.value } : null,
);

const canSubmit = computed(() => bytes.value !== null);

watch(open, (v) => {
  if (v) {
    filename.value = '';
    fileSize.value = 0;
    bytes.value = null;
    error.value = null;
    submitting.value = false;
    if (fileInput.value) fileInput.value.value = '';
  }
});

function onFile(e: Event) {
  const f = (e.target as HTMLInputElement).files?.[0];
  if (!f) return;
  filename.value = f.name;
  fileSize.value = f.size;
  const reader = new FileReader();
  reader.onload = () => {
    const buf = reader.result;
    if (buf instanceof ArrayBuffer) {
      bytes.value = Array.from(new Uint8Array(buf));
    } else {
      error.value = '读文件失败';
    }
  };
  reader.onerror = () => { error.value = '读文件失败'; };
  reader.readAsArrayBuffer(f);
}

async function onSubmit() {
  if (bytes.value === null) return;
  error.value = null;
  submitting.value = true;
  try {
    emit('submit', { filename: filename.value, bytes: bytes.value });
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
.row input[type=file] {
  flex: 1;
}
.file-info {
  font-size: 12px;
  color: var(--text-muted);
  margin-bottom: 12px;
}
.error {
  color: var(--danger);
  font-size: 12px;
  margin-bottom: 8px;
}
</style>
