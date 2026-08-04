<template>
  <Dialog v-model:open="open" title="创建转换小说" :width="420">
    <div class="row">
      <label>源 upload</label>
      <span class="hint">id {{ dataAssetId }} · 已解析</span>
    </div>
    <div class="row">
      <label>标题 *</label>
      <Input v-model="title" placeholder="如:斗破_热血版" class="title-input" />
    </div>
    <div class="row">
      <label>默认模型</label>
      <input
        class="default-model-input number"
        type="number"
        :value="defaultModelConfigId ?? ''"
        placeholder="可空"
        @input="onDefaultModelInput"
      />
    </div>
    <div class="row">
      <label>默认 prompt</label>
      <input
        class="default-prompt-input number"
        type="number"
        :value="defaultPromptId ?? ''"
        placeholder="可空"
        @input="onDefaultPromptInput"
      />
    </div>
    <div class="row">
      <label>默认模式</label>
      <select class="default-mode-select" :value="defaultMode ?? ''" @change="onDefaultModeChange">
        <option value="">（未选）</option>
        <option value="compress">compress · 压缩</option>
        <option value="style">style · 文风</option>
      </select>
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <template #footer>
      <Button @click="open = false">取消</Button>
      <Button kind="primary" class="submit" :disabled="title.trim() === '' || submitting" @click="onSubmit">创建</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import Input from './ui/Input.vue';
import type { TransformMode } from '../ipc/types';

const props = defineProps<{ dataAssetId: number }>();
const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{
  submit: [{
    data_asset_id: number;
    title: string;
    default_model_config_id: number | null;
    default_prompt_id: number | null;
    default_mode: TransformMode | null;
  }];
}>();

const title = ref('');
const defaultModelConfigId = ref<number | null>(null);
const defaultPromptId = ref<number | null>(null);
const defaultMode = ref<TransformMode | null>(null);
const error = ref<string | null>(null);
const submitting = ref(false);

watch(open, (v) => {
  if (v) {
    title.value = '';
    defaultModelConfigId.value = null;
    defaultPromptId.value = null;
    defaultMode.value = null;
    error.value = null;
    submitting.value = false;
  }
});

function onDefaultModelInput(e: Event) {
  const raw = (e.target as HTMLInputElement).value;
  defaultModelConfigId.value = raw === '' ? null : Number(raw);
}

function onDefaultPromptInput(e: Event) {
  const raw = (e.target as HTMLInputElement).value;
  defaultPromptId.value = raw === '' ? null : Number(raw);
}

function onDefaultModeChange(e: Event) {
  const raw = (e.target as HTMLSelectElement).value;
  defaultMode.value = raw === '' ? null : (raw as TransformMode);
}

async function onSubmit() {
  error.value = null;
  submitting.value = true;
  try {
    emit('submit', {
      data_asset_id: props.dataAssetId,
      title: title.value.trim(),
      default_model_config_id: defaultModelConfigId.value,
      default_prompt_id: defaultPromptId.value,
      default_mode: defaultMode.value,
    });
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
.row .number,
.row select {
  flex: 1;
  height: 34px;
  padding: 6px 12px;
  border: none;
  border-bottom: 1px solid var(--border-color);
  background: transparent;
  font-family: var(--font-sans);
  font-size: 14px;
  color: var(--text-primary);
  outline: none;
  box-sizing: border-box;
}
.row .number::placeholder { color: var(--text-muted); }
.hint { font-size: 13px; color: var(--text-muted); }
.error { color: var(--danger); font-size: 12px; }
</style>
