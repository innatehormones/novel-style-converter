<template>
  <Dialog v-model:open="open" title="新建工作流" :width="880">
    <div class="dialog-grid">
      <div class="left-col">
        <div class="summary">
          已选 <strong>{{ selectedChapterIds.length }}</strong> 章
        </div>
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
          <label>标签 *</label>
          <input v-model="label" placeholder="如 'v1 全量'" class="label-input" :class="{ 'has-error': labelError !== null }" @input="labelError = null" />
        </div>
        <div v-if="labelError" class="row-error">{{ labelError }}</div>
        <div class="row policy-row">
          <label>失败策略 *</label>
          <div class="policy-options">
            <label class="policy-opt" data-policy="pause_and_review" :class="{ 'is-active': onFailurePolicy === 'pause_and_review' }">
              <input type="radio" v-model="onFailurePolicy" value="pause_and_review" />
              <span class="policy-icon" aria-hidden="true"><IconPauseCircle /></span>
              <span class="policy-body">
                <strong>暂停与审阅</strong>
                <small>失败时停下来,等你决定</small>
              </span>
              <span class="policy-radio" aria-hidden="true" />
            </label>
            <label class="policy-opt" data-policy="skip_failed" :class="{ 'is-active': onFailurePolicy === 'skip_failed' }">
              <input type="radio" v-model="onFailurePolicy" value="skip_failed" />
              <span class="policy-icon" aria-hidden="true"><IconSkipForward /></span>
              <span class="policy-body">
                <strong>跳过问题章节</strong>
                <small>失败章节跳过,继续派下一章</small>
              </span>
              <span class="policy-radio" aria-hidden="true" />
            </label>
          </div>
        </div>
        <div class="hint">
          默认从转换小说继承 prompt / model;此处覆盖后仅作用于本次工作流。
        </div>
      </div>
      <div class="right-col">
        <div class="ctx-section">
          <h3 class="section-title">上下文</h3>
          <div class="ctx-toggles">
            <label class="toggle">
              <input type="checkbox" v-model="includePrev" />
              <span>带前文</span>
              <small>前文原文 + 前文转换</small>
            </label>
            <label class="toggle">
              <input type="checkbox" v-model="includeNext" />
              <span>带后文</span>
              <small>后文原文</small>
            </label>
          </div>
        </div>
        <!-- 试运行区占位 —— Task 7 填充 -->
        <div class="preview-placeholder">试运行区(TODO Task 7)</div>
      </div>
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <template #footer>
      <Button @click="open = false">取消</Button>
      <Button
        kind="primary"
        :loading="submitting"
        :disabled="!canSubmit"
        @click="onSubmit"
      >⚙ 创建</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import IconPauseCircle from '~icons/lucide/pause-circle';
import IconSkipForward from '~icons/lucide/skip-forward';
import { listModels, listPrompts } from '../ipc/commands';
import type { ModelConfig, Prompt, CreateWorkflowInput } from '../ipc/types';

const props = defineProps<{
  tnId: number;
  defaultPromptId?: number | null;
  defaultModelConfigId?: number | null;
  defaultMode?: 'compress' | 'style' | null;
  selectedChapterIds: number[];
}>();
const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{
  submit: [CreateWorkflowInput];
}>();

const prompts = ref<Prompt[]>([]);
const models = ref<ModelConfig[]>([]);
const promptId = ref(0);
const modelConfigId = ref(0);
const label = ref('');
const labelError = ref<string | null>(null);
const includePrev = ref(false);
const includeNext = ref(false);
const onFailurePolicy = ref<CreateWorkflowInput['on_failure_policy']>('pause_and_review');
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
  label.value.trim() !== '' &&
  props.selectedChapterIds.length > 0 &&
  !submitting.value,
);

watch(open, async (v) => {
  if (!v) return;
  error.value = null;
  submitting.value = false;
  promptId.value = props.defaultPromptId ?? 0;
  modelConfigId.value = props.defaultModelConfigId ?? 0;
  label.value = '';
  labelError.value = null;
  includePrev.value = false;
  includeNext.value = false;
  onFailurePolicy.value = 'pause_and_review';
  try {
    const [pRes, mRes] = await Promise.all([listPrompts(), listModels()]);
    prompts.value = pRes;
    models.value = mRes;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}, { immediate: true });

async function onSubmit() {
  if (!canSubmit.value) {
    if (label.value.trim() === '') labelError.value = '请填写标签';
    return;
  }
  // mode 由所选 prompt 的 kind 决定(后端 create_workflow 会再次校验)。
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
      tn_id: props.tnId,
      label: label.value.trim(),
      chapter_ids: [...props.selectedChapterIds],
      prompt_id: promptId.value,
      model_config_id: modelConfigId.value,
      mode,
      ctx_prev_original: includePrev.value ? 1 : 0,
      ctx_prev_transformed: includePrev.value ? 1 : 0,
      ctx_next_original: includeNext.value ? 1 : 0,
      on_failure_policy: onFailurePolicy.value,
      // 试运行首章结果(spec §3.1):Task 6 dialog 改造时改成绑定 previewFirstChapter() 的输出。
      preview_first_chapter: null,
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
.summary {
  margin-bottom: 12px;
  padding: 10px 14px;
  background: var(--bg-section);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  font-size: 14px;
  color: var(--text-secondary);
}
.summary strong {
  color: var(--color-cinnabar);
  font-size: 16px;
  font-family: var(--font-mono);
  margin: 0 4px;
}
.row { display: flex; align-items: center; margin-bottom: 12px; gap: 12px; }
.row > label { width: 100px; font-size: 14px; color: var(--text-secondary); flex-shrink: 0; }
.row select, .row input { flex: 1; height: 32px; }
.label-input {
  padding: 0 8px; border: 1px solid var(--border-color); border-radius: var(--radius-pin);
  background: var(--color-sheet); color: var(--text-primary);
}
.label-input.has-error { border-color: var(--danger); }
.row-error { color: var(--danger); font-size: 12px; margin: -8px 0 12px 112px; }
.error { color: var(--danger); font-size: 12px; margin-top: 8px; }
.hint { color: var(--text-muted); font-size: 12px; margin-top: 8px; line-height: 1.5; }
.dialog-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
  min-height: 460px;
}
.left-col, .right-col {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.right-col .summary { margin-bottom: 0; }
.section-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  margin: 0 0 4px 0;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--border-soft);
}
.ctx-toggles { display: flex; flex-direction: column; gap: 8px; }
.toggle {
  display: flex; flex-direction: column; gap: 2px; cursor: pointer;
  padding: 8px 12px;
  border: 1px solid var(--border-soft); border-radius: var(--radius-pin);
  background: var(--color-sheet);
  transition: border-color 0.15s, background 0.15s;
}
.toggle:hover { border-color: var(--border-color); background: var(--bg-hover); }
.toggle:has(input:checked) {
  border-color: var(--color-cinnabar);
  background: var(--color-cinnabar-light);
}
.toggle > span { font-size: 13px; font-weight: 600; color: var(--text-primary); }
.toggle > small { font-size: 11px; color: var(--text-muted); }
.toggle input[type='checkbox'] {
  position: absolute; opacity: 0; pointer-events: none; width: 0; height: 0;
}
.preview-placeholder {
  flex: 1;
  display: flex; align-items: center; justify-content: center;
  border: 1px dashed var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--bg-section);
  color: var(--text-muted);
  font-size: 12px;
  padding: 16px;
}
.row.policy-row { align-items: flex-start; }
.policy-options { display: flex; flex-direction: column; gap: 8px; flex: 1; }
.policy-opt {
  display: flex; align-items: center; gap: 12px; cursor: pointer;
  font-size: 13px; padding: 10px 14px;
  border: 1px solid var(--border-soft); border-radius: var(--radius-pin);
  background: var(--color-sheet);
  transition: border-color 0.15s ease, background 0.15s ease, box-shadow 0.15s ease;
}
.policy-opt:hover {
  border-color: var(--border-color);
  background: var(--bg-hover);
}
.policy-opt input[type='radio'] {
  position: absolute; opacity: 0; pointer-events: none; width: 0; height: 0;
}
.policy-icon {
  display: inline-flex; align-items: center; justify-content: center;
  width: 28px; height: 28px;
  border-radius: 50%;
  background: var(--bg-section);
  color: var(--text-muted);
  flex-shrink: 0;
  transition: background 0.15s ease, color 0.15s ease;
}
.policy-icon svg { width: 16px; height: 16px; }
.policy-body { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
.policy-body strong { color: var(--text-primary); font-weight: 600; line-height: 1.3; }
.policy-body small { color: var(--text-muted); font-size: 11px; line-height: 1.4; }
.policy-radio {
  width: 16px; height: 16px; border-radius: 50%;
  border: 2px solid var(--border-color);
  flex-shrink: 0;
  position: relative;
  box-sizing: border-box;
  transition: border-color 0.15s ease;
}
.policy-opt.is-active {
  border-color: var(--color-cinnabar);
  background: var(--color-cinnabar-light);
  box-shadow: 0 0 0 1px var(--color-cinnabar) inset;
}
.policy-opt.is-active .policy-radio { border-color: var(--color-cinnabar); }
.policy-opt.is-active .policy-radio::after {
  content: ''; position: absolute;
  top: 2px; left: 2px;
  width: 8px; height: 8px; border-radius: 50%;
  background: var(--color-cinnabar);
}
.policy-opt.is-active .policy-icon { background: var(--color-sheet); }
.policy-opt.is-active[data-policy='pause_and_review'] .policy-icon { color: var(--color-tangerine); }
.policy-opt.is-active[data-policy='skip_failed'] .policy-icon { color: var(--color-vivid-green); }
</style>
