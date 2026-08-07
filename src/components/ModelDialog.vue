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
    <div class="row hint-row">
      <span class="concurrency-hint">per-model 信号量大小，worker 端按此限流。物理 worker 数默认为 2。</span>
    </div>
    <div class="row actions">
      <Button :loading="store.testing" @click="onTest">测试连接</Button>
      <Button size="small" @click="onClearReport" v-if="testReport">清空结果</Button>
    </div>
    <div v-if="testReport" class="report" :class="{ ok: !testReport.error, fail: !!testReport.error }">
      <div class="report-head">
        <Tag :kind="testReport.error ? 'danger' : 'success'">
          {{ testReport.error ? '失败' : '成功' }}
        </Tag>
        <span class="metric"><strong>{{ testReport.latency_ms }}</strong> ms</span>
        <span class="metric" v-if="testReport.tokens_in != null">
          tokens in <strong>{{ testReport.tokens_in }}</strong>
        </span>
        <span class="metric" v-if="testReport.tokens_out != null">
          tokens out <strong>{{ testReport.tokens_out }}</strong>
        </span>
      </div>
      <pre v-if="testReport.content_preview" class="preview">{{ testReport.content_preview }}</pre>
      <pre v-if="testReport.error" class="error-text">{{ testReport.error }}</pre>
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
import type { ModelConfigInput, TestModelReport } from '../ipc/types';

const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{ submit: [ModelConfigInput] }>();
const props = defineProps<{ initial: ModelConfigInput | null }>();

const store = useModelsStore();
const editing = computed(() => (props.initial?.id ?? 0) > 0);
const form = reactive<ModelConfigInput>(blank());
const maxTokensRef = ref<number | null>(null);
const temperatureRef = ref<number | null>(null);
const testReport = ref<TestModelReport | null>(null);

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
    testReport.value = null;
    return;
  }
  Object.assign(form, value);
  maxTokensRef.value = value.max_tokens ?? null;
  temperatureRef.value = value.temperature ?? null;
  testReport.value = null;
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
  testReport.value = null;
  try {
    testReport.value = await store.test({
      ...form,
      max_tokens: maxTokensRef.value,
      temperature: temperatureRef.value,
    });
  } catch (e) {
    // 后端几乎不会到这里（report.error 已承载）；只防 IPC 通道异常。
    testReport.value = {
      model: form.model,
      base_url: form.base_url,
      latency_ms: 0,
      tokens_in: null,
      tokens_out: null,
      content_preview: null,
      error: e instanceof Error ? e.message : String(e),
    };
  }
}

function onClearReport() {
  testReport.value = null;
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
.hint-row {
  margin-top: -8px;
  margin-bottom: 12px;
}
.concurrency-hint {
  margin-left: 112px;
  font-size: 11px;
  color: var(--text-muted);
  font-style: italic;
}
.report {
  margin-top: 12px;
  padding: 10px 12px;
  border-radius: var(--radius-pin);
  border: 1px solid var(--border-soft);
  background: var(--color-sheet);
  font-family: var(--font-mono);
  font-size: 12px;
}
.report.ok {
  border-color: var(--success-border, var(--border-soft));
  background: var(--success-bg, var(--color-sheet));
}
.report.fail {
  border-color: var(--danger-border);
  background: var(--danger-bg);
}
.report-head {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.metric {
  color: var(--text-secondary);
}
.metric strong {
  color: var(--text-primary);
  font-family: var(--font-mono);
  font-weight: 600;
  margin: 0 2px;
}
.preview {
  margin: 8px 0 0;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--text-primary);
  max-height: 160px;
  overflow: auto;
}
.error-text {
  margin: 8px 0 0;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--danger);
  max-height: 200px;
  overflow: auto;
}
</style>

