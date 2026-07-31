<template>
  <Dialog v-model:open="open" :title="editing ? '编辑模型' : '新增模型'" :width="540">
    <div class="row">
      <label>名称 *</label>
      <Input v-model="form.name" />
    </div>
    <div class="row">
      <label>Base URL *</label>
      <Input v-model="form.base_url" placeholder="https://api.openai.com/v1" />
    </div>
    <div class="row">
      <label>API Key *</label>
      <Input v-model="form.api_key" type="password" />
    </div>
    <div class="row">
      <label>模型名 *</label>
      <Input v-model="form.model" placeholder="gpt-4" />
    </div>
    <div class="row">
      <label>max_tokens</label>
      <NumberInput v-model="maxTokensRef" :min="0" :step="256" />
    </div>
    <div class="row">
      <label>temperature</label>
      <NumberInput v-model="temperatureRef" :min="0" :max="2" :step="0.1" />
    </div>
    <div class="row">
      <label>并发数</label>
      <NumberInput v-model="form.concurrency" :min="1" :max="16" />
    </div>
    <div class="row actions">
      <Button :loading="store.testing" @click="onTest">测试连接</Button>
      <Tag v-if="testResult" kind="success" style="margin-left: 8px">
        成功:{{ testResult }}
      </Tag>
      <Tag v-if="testError" kind="danger" style="margin-left: 8px">
        失败:{{ testError }}
      </Tag>
    </div>
    <template #footer>
      <Button @click="open = false">取消</Button>
      <Button kind="primary" :disabled="!canSubmit" @click="onSubmit">
        保存
      </Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import Input from './ui/Input.vue';
import NumberInput from './ui/NumberInput.vue';
import Tag from './ui/Tag.vue';
import { useModelsStore } from '../stores/models';
import type { ModelConfigInput } from '../ipc/types';

const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{ submit: [ModelConfigInput] }>();
const props = defineProps<{ initial: ModelConfigInput | null }>();

const store = useModelsStore();
const editing = computed(() => (props.initial?.id ?? 0) > 0);
const form = reactive<ModelConfigInput>(blank());
const maxTokensRef = ref<number | null>(null);
const temperatureRef = ref<number | null>(null);
const testResult = ref<string | null>(null);
const testError = ref<string | null>(null);

function blank(): ModelConfigInput {
  return {
    id: 0,
    name: '',
    base_url: 'https://api.openai.com/v1',
    api_key: '',
    model: '',
    max_tokens: null,
    temperature: null,
    concurrency: 1,
  };
}

function applyInitial(value: ModelConfigInput | null) {
  if (!value) {
    Object.assign(form, blank());
    maxTokensRef.value = null;
    temperatureRef.value = null;
    testResult.value = null;
    testError.value = null;
    return;
  }
  Object.assign(form, value);
  maxTokensRef.value = value.max_tokens ?? null;
  temperatureRef.value = value.temperature ?? null;
  testResult.value = null;
  testError.value = null;
}

watch(() => props.initial, applyInitial, { immediate: true });
watch(open, (v) => {
  if (v) applyInitial(props.initial);
});

const canSubmit = computed(
  () =>
    form.name.trim() !== '' &&
    form.base_url.trim() !== '' &&
    form.api_key.trim() !== '' &&
    form.model.trim() !== '',
);

async function onTest() {
  testResult.value = null;
  testError.value = null;
  try {
    const content = await store.test({
      ...form,
      max_tokens: maxTokensRef.value,
      temperature: temperatureRef.value,
    });
    testResult.value = content.slice(0, 60);
  } catch (e) {
    testError.value = e instanceof Error ? e.message : String(e);
  }
}

function onSubmit() {
  if (!canSubmit.value) return;
  emit('submit', {
    ...form,
    max_tokens: maxTokensRef.value,
    temperature: temperatureRef.value,
  });
  open.value = false;
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
  width: 100px;
  font-size: 14px;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.row.actions {
  margin-top: 16px;
}
</style>
