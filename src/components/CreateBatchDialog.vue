<template>
  <Dialog v-model:open="open" title="新建工作流" :width="540">
    <div class="row">
      <label>提示词模板 *</label>
      <select v-model="promptId" class="prompt-select">
        <option :value="0" disabled>{{ prompts.length === 0 ? '加载中...' : '选择 prompt...' }}</option>
        <option v-for="p in filteredPrompts" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
    </div>
    <div class="row">
      <label>模型配置 *</label>
      <select v-model="modelConfigId" class="model-select">
        <option :value="0" disabled>{{ models.length === 0 ? '加载中...' : '选择 model...' }}</option>
        <option v-for="m in models" :key="m.id" :value="m.id">{{ m.name }} ({{ m.model }})</option>
      </select>
    </div>
    <div class="row">
      <label>失败策略 *</label>
      <select v-model="policy" class="policy-select">
        <option value="pause_and_review">失败时暂停,人工介入</option>
        <option value="terminate">失败时终止整批</option>
        <option value="skip_failed">失败时跳过该章</option>
      </select>
    </div>
    <div class="row">
      <label>标签</label>
      <input v-model="label" placeholder="可选,如 'v1 全量'" class="label-input" />
    </div>
    <div class="row ctx">
      <div>
        <label>前文原文</label>
        <NumberInput v-model="ctxPrevOriginal" :min="0" :max="20" />
      </div>
      <div>
        <label>前文转换</label>
        <NumberInput v-model="ctxPrevTransformed" :min="0" :max="20" />
      </div>
      <div>
        <label>后文原文</label>
        <NumberInput v-model="ctxNextOriginal" :min="0" :max="20" />
      </div>
    </div>
    <div class="ctx-hint">
      给 LLM 的上下文窗口大小（章）。一般只设"前文转换" 1~3,
      让模型参考前面已经转换好的章节学文风;原文带多了浪费 token。
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <div class="hint">
      默认从转换小说继承 prompt / model / mode;此处覆盖后仅作用于本次工作流。
      该工作流将处理转换小说下全部章节。
    </div>
    <template #footer>
      <Button @click="open = false">取消</Button>
      <Button
        kind="primary"
        :loading="submitting"
        :disabled="!canSubmit"
        @click="onSubmit"
      >⚙ 创建并运行</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import NumberInput from './ui/NumberInput.vue';
import { listModels, listPrompts } from '../ipc/commands';
import type { ModelConfig, OnFailurePolicy, Prompt } from '../ipc/types';

const props = defineProps<{
  tnId: number;
  defaultPromptId?: number | null;
  defaultModelConfigId?: number | null;
  defaultMode?: 'compress' | 'style' | null;
}>();
const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{
  submit: [{
    label: string | null;
    on_failure_policy: OnFailurePolicy;
    overrides: {
      prompt_id: number;
      model_config_id: number;
      mode: 'compress' | 'style';
      ctx_prev_original: number;
      ctx_prev_transformed: number;
      ctx_next_original: number;
    };
  }];
}>();

const prompts = ref<Prompt[]>([]);
const models = ref<ModelConfig[]>([]);
const promptId = ref(0);
const modelConfigId = ref(0);
const policy = ref<OnFailurePolicy>('pause_and_review');
const label = ref('');
const ctxPrevOriginal = ref<number | null>(0);
const ctxPrevTransformed = ref<number | null>(0);
const ctxNextOriginal = ref<number | null>(0);
const submitting = ref(false);
const error = ref<string | null>(null);

const filteredPrompts = computed(() => {
  if (!props.defaultMode) return prompts.value;
  const want = props.defaultMode === 'compress' ? 'compress' : 'style';
  return prompts.value.filter((p) => p.kind === want);
});

const canSubmit = computed(() =>
  promptId.value !== 0 &&
  modelConfigId.value !== 0 &&
  ctxPrevOriginal.value !== null &&
  ctxPrevTransformed.value !== null &&
  ctxNextOriginal.value !== null &&
  !submitting.value,
);

watch(open, async (v) => {
  if (!v) return;
  error.value = null;
  submitting.value = false;
  promptId.value = props.defaultPromptId ?? 0;
  modelConfigId.value = props.defaultModelConfigId ?? 0;
  label.value = '';
  ctxPrevOriginal.value = 0;
  ctxPrevTransformed.value = 0;
  ctxNextOriginal.value = 0;
  try {
    const [pRes, mRes] = await Promise.all([listPrompts(), listModels()]);
    prompts.value = pRes;
    models.value = mRes;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}, { immediate: true });

async function onSubmit() {
  if (!canSubmit.value) return;
  // mode 由所选 prompt 的 kind 决定(后端 BatchOverrides 也接受 mode 字符串)。
  const selectedPrompt = prompts.value.find((p) => p.id === promptId.value);
  const mode = selectedPrompt?.kind;
  if (mode !== 'compress' && mode !== 'style') {
    error.value = '请选择一个 prompt 以确定 mode。';
    return;
  }
  submitting.value = true;
  error.value = null;
  try {
    emit('submit', {
      label: label.value.trim() === '' ? null : label.value.trim(),
      on_failure_policy: policy.value,
      overrides: {
        prompt_id: promptId.value,
        model_config_id: modelConfigId.value,
        mode,
        ctx_prev_original: ctxPrevOriginal.value ?? 0,
        ctx_prev_transformed: ctxPrevTransformed.value ?? 0,
        ctx_next_original: ctxNextOriginal.value ?? 0,
      },
    });
    open.value = false;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    submitting.value = false;
  }
}
</script>

<style scoped>
.row { display: flex; align-items: center; margin-bottom: 12px; gap: 12px; }
.row > label { width: 100px; font-size: 14px; color: var(--text-secondary); flex-shrink: 0; }
.row select, .row input { flex: 1; height: 32px; }
.label-input { padding: 0 8px; border: 1px solid var(--border-color); border-radius: var(--radius-pin); background: var(--color-sheet); color: var(--text-primary); }
.row.ctx { gap: 16px; }
.row.ctx > div { flex: 1; display: flex; flex-direction: column; gap: 4px; }
.row.ctx label { width: auto; font-size: 12px; color: var(--text-muted); }
.error { color: var(--danger); font-size: 12px; margin-top: 8px; }
.hint { color: var(--text-muted); font-size: 12px; margin-top: 8px; line-height: 1.5; }
.ctx-hint { color: var(--text-muted); font-size: 11px; margin-top: -4px; margin-bottom: 12px; line-height: 1.5; }
</style>