<template>
  <Dialog v-model:open="open" title="补充章节" :width="880">
    <div class="acd">
      <!-- 顶部 context strip:展示目标 batch 的 prompt / model / mode / 上下文设置,作"我往哪个工作流补"提示 -->
      <header class="context-strip">
        <div class="ctx-row">
          <span class="ctx-label">工作流</span>
          <span class="ctx-value">#{{ batchId }}</span>
        </div>
        <div class="ctx-row">
          <span class="ctx-label">提示词</span>
          <span class="ctx-value">{{ promptName }}</span>
        </div>
        <div class="ctx-row">
          <span class="ctx-label">模型</span>
          <span class="ctx-value">{{ modelDisplayName }}</span>
        </div>
        <div class="ctx-row">
          <span class="ctx-label">模式</span>
          <span class="ctx-value">{{ mode === 'compress' ? '压缩' : '改写' }}</span>
        </div>
        <div class="ctx-row ctx-toggles">
          <span class="ctx-label">上下文</span>
          <span class="ctx-value">
            前文原文 × {{ ctxPrevOriginal }} · 前文转换 × {{ ctxPrevTransformed }} · 后文原文 × {{ ctxNextOriginal }}
          </span>
        </div>
      </header>

      <!-- 选择区 -->
      <section class="picker">
        <div class="picker-summary">
          已选 <strong>{{ selectedChapterIds.size }}</strong> 章
          · 共 {{ availableSources.length }} 章可补充
          · 已在 batch 中 {{ existingChapterIds.size }} 章(不可重复选)
        </div>
        <div class="range-pick">
          <span class="range-label">按 # 选</span>
          <input
            type="number"
            class="range-input"
            :class="{ 'has-error': rangeError !== null }"
            v-model.number="rangeFrom"
            :min="1"
            :max="allSources.length"
            placeholder="起"
            :disabled="allSources.length === 0"
            @keydown.enter="applyRange"
          />
          <span class="range-sep">—</span>
          <input
            type="number"
            class="range-input"
            :class="{ 'has-error': rangeError !== null }"
            v-model.number="rangeTo"
            :min="1"
            :max="allSources.length"
            placeholder="止"
            :disabled="allSources.length === 0"
            @keydown.enter="applyRange"
          />
          <select
            v-model="rangeMode"
            class="range-mode"
            :disabled="allSources.length === 0"
            :title="rangeMode === 'toggle' ? '范围内每项取反选中状态,范围外不变 — 多次应用可累加' : '直接把选中集合替换为范围内的章节'"
          >
            <option value="toggle">累加/取反</option>
            <option value="replace">覆盖</option>
          </select>
          <Button
            size="small"
            :disabled="rangeFrom === null || rangeTo === null || rangeError !== null"
            :title="rangeError ?? ''"
            @click="applyRange"
          >应用</Button>
          <Button
            size="small"
            :disabled="selectedChapterIds.size === 0"
            @click="clearSelection"
          >清空</Button>
        </div>

        <!-- loading / error -->
        <div v-if="loadErrorMessage" class="error">{{ loadErrorMessage }}</div>
        <div v-else-if="isLoading" class="loading">加载章节中...</div>
        <div v-else-if="availableSources.length === 0" class="empty">该工作流已经覆盖本转换工程下的全部章节,无可补充项。</div>

        <!-- 章节列表:渲染 availableSources(idx ASC),已 disabled 的章节(已在 batch 中)显示在末尾,标"已在 batch 中" -->
        <div v-else class="chapter-list">
          <div
            v-for="row in rowsForDisplay"
            :key="row.chapter_id"
            class="row"
            :class="{ 'is-selected': row.inBatch ? false : selectedChapterIds.has(row.chapter_id), 'in-batch': row.inBatch }"
          >
            <input
              type="checkbox"
              :checked="row.inBatch ? false : selectedChapterIds.has(row.chapter_id)"
              :disabled="row.inBatch"
              :title="row.inBatch ? '已在该工作流中,不可重复选' : ''"
              @change="(e) => toggleSelect(row.chapter_id, (e.target as HTMLInputElement).checked)"
            />
            <span class="idx">#{{ row.idx }}</span>
            <span class="title">{{ row.title }}</span>
            <span class="words">{{ row.word_count.toLocaleString('zh-Hans-CN') }} 字</span>
            <span v-if="row.inBatch" class="badge">已在 batch 中</span>
          </div>
        </div>
      </section>
    </div>

    <template #footer>
      <Button @click="open = false">取消</Button>
      <Button
        kind="primary"
        :loading="submitting"
        :disabled="selectedChapterIds.size === 0 || submitting"
        @click="onConfirm"
      >补充 {{ selectedChapterIds.size }} 章</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useQuery } from '@tanstack/vue-query';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import { listTransformationSourceChapters, listWorkflowChapters } from '../ipc/commands';
import type { SourceChapterRow, WorkflowChapterRow } from '../ipc/types';

/// 「补充章节」对话框 spec(stopped-batch-append-chapters):
/// - 目标 batch 已 stopped,用户从这里选若干 source 章节追加进去。
/// - 已在 batch 中的章节不允许重复选(显示但 disabled,标记「已在 batch 中」)。
/// - 章节选择 UI 复用章节来源 tab 的 rangeFrom/To + rangeMode(replace/toggle),逻辑从
///   TransformationNovelDetail.vue 复制 —— Tasks 8/9 之后会一起抽。
const props = defineProps<{
  batchId: number;
  transformationNovelId: number;
  promptName: string;
  modelDisplayName: string;
  mode: 'compress' | 'style';
  ctxPrevOriginal: number;
  ctxPrevTransformed: number;
  ctxNextOriginal: number;
}>();

const open = defineModel<boolean>('open', { required: true });

const emit = defineEmits<{
  confirm: [{ batchId: number; chapterIds: number[] }];
}>();

const selectedChapterIds = ref<Set<number>>(new Set());
const submitting = ref(false);

/// 源章节列表 —— 与 TransformationNovelDetail.vue 共用 queryKey,自动共享缓存 + 实时同步。
const sourcesQuery = useQuery({
  queryKey: ['transformationSourceChapters', props.transformationNovelId],
  queryFn: () => listTransformationSourceChapters(props.transformationNovelId),
});
const allSources = computed<SourceChapterRow[]>(() => sourcesQuery.data.value ?? []);

/// 当前 batch 已包含的章节 —— vue-query 按 [batchId] 缓存,跟 TransformationNovelDetail 里的
/// selectedWorkflowChaptersQuery 是同一个 key(命中已存在的缓存,这里不再发起请求)。
const workflowChaptersQuery = useQuery({
  queryKey: ['workflowChapters', props.batchId],
  queryFn: () => listWorkflowChapters(props.batchId),
});
const batchChapters = computed<WorkflowChapterRow[]>(() => workflowChaptersQuery.data.value ?? []);

/// 已存在于该 batch 的 chapter_id 集合 —— 用于过滤掉不可重复选的章节。
const existingChapterIds = computed<Set<number>>(
  () => new Set(batchChapters.value.map((c) => c.chapter_id)),
);

/// 可补充的章节 = 全部 source - 已存在 batch 的章节。
const availableSources = computed<SourceChapterRow[]>(() => {
  const ex = existingChapterIds.value;
  return allSources.value.filter((s) => !ex.has(s.chapter_id));
});

const isLoading = computed<boolean>(
  () => sourcesQuery.isLoading.value || workflowChaptersQuery.isLoading.value,
);

/// vue-query 的 error 是响应式 Ref;在 template 里直接展示两路任一错误即可。
const loadErrorMessage = computed<string | null>(() => {
  const err = sourcesQuery.error.value ?? workflowChaptersQuery.error.value;
  if (!err) return null;
  return err instanceof Error ? err.message : String(err);
});

/// 章节显示列表:可补充的排前面(idx ASC),已存在的 batch 中章节追加在末尾(disabled + badge)。
type DisplayRow = SourceChapterRow & { inBatch: boolean };
const rowsForDisplay = computed<DisplayRow[]>(() => [
  ...availableSources.value.map((s) => ({ ...s, inBatch: false })),
  ...allSources.value
    .filter((s) => existingChapterIds.value.has(s.chapter_id))
    .map((s) => ({ ...s, inBatch: true })),
]);

function toggleSelect(chapterId: number, on: boolean) {
  const next = new Set(selectedChapterIds.value);
  if (on) next.add(chapterId); else next.delete(chapterId);
  selectedChapterIds.value = next;
}

function clearSelection() {
  selectedChapterIds.value = new Set();
}

// range 选择 —— 与 TransformationNovelDetail.vue 同形代码。Task 8/9 后整体抽。
// 注意:idx 是全 source 列表的序号(1..allSources.length),不是 availableSources 的序号。
// rangeError 需按全源数量校验(用户视角的"第 10 章"),applyRange 过滤时再跳过已存在 batch 的。
type RangeMode = 'replace' | 'toggle';
const rangeMode = ref<RangeMode>('replace');
const rangeFrom = ref<number | null>(null);
const rangeTo = ref<number | null>(null);
const rangeError = computed<string | null>(() => {
  const total = allSources.value.length;
  if (total === 0) return null;
  if (rangeFrom.value === null || rangeTo.value === null) return null;
  if (rangeFrom.value < 1 || rangeTo.value < 1) return '序号需 ≥ 1';
  if (rangeFrom.value > total || rangeTo.value > total) {
    return `序号需 ≤ ${total}`;
  }
  return null;
});
function applyRange() {
  const list = availableSources.value;
  if (list.length === 0) return;
  if (rangeError.value !== null) return;
  if (rangeFrom.value === null || rangeTo.value === null) return;
  const lo = Math.min(rangeFrom.value, rangeTo.value);
  const hi = Math.max(rangeFrom.value, rangeTo.value);
  const targetIds = new Set<number>(
    list.filter((s) => s.idx >= lo && s.idx <= hi).map((s) => s.chapter_id),
  );
  if (rangeMode.value === 'replace') {
    selectedChapterIds.value = targetIds;
  } else {
    const next = new Set(selectedChapterIds.value);
    for (const id of targetIds) {
      if (next.has(id)) next.delete(id); else next.add(id);
    }
    selectedChapterIds.value = next;
  }
}

watch(open, (v) => {
  if (!v) return;
  submitting.value = false;
  selectedChapterIds.value = new Set();
  rangeFrom.value = null;
  rangeTo.value = null;
  rangeMode.value = 'replace';
  // sources / batchChapters 由 vue-query 自动订阅 + 自动重试;loadErrorMessage 是
  // computed,会自动跟着 query 的 error 状态变化,无需在这里手动同步。
});

function onConfirm() {
  if (selectedChapterIds.value.size === 0) return;
  if (submitting.value) return;
  submitting.value = true;
  try {
    emit('confirm', {
      batchId: props.batchId,
      chapterIds: [...selectedChapterIds.value],
    });
    // 不在这里关 dialog —— 父组件成功后再关,失败保留选项让用户改。
  } finally {
    submitting.value = false;
  }
}
</script>

<style scoped>
.acd {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-height: 380px;
}
.context-strip {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px 14px;
  background: var(--bg-section);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
}
.ctx-row {
  display: flex;
  align-items: baseline;
  gap: 10px;
  font-size: 13px;
}
.ctx-label {
  width: 56px;
  color: var(--text-muted);
  font-size: 12px;
  flex-shrink: 0;
}
.ctx-value {
  color: var(--text-primary);
  font-family: var(--font-mono);
}
.picker-summary {
  padding: 10px 14px;
  background: var(--bg-section);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  font-size: 13px;
  color: var(--text-secondary);
}
.picker-summary strong {
  color: var(--color-cinnabar);
  font-size: 15px;
  font-family: var(--font-mono);
  margin: 0 4px;
}
.range-pick {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
.range-label {
  font-size: 12px;
  color: var(--text-muted);
  margin-right: 2px;
}
.range-input {
  width: 56px;
  height: 28px;
  padding: 0 6px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
  color: var(--text-primary);
  font-family: var(--font-mono);
  font-size: 12px;
  box-sizing: border-box;
}
.range-input.has-error {
  border-color: var(--danger);
}
.range-input:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
.range-sep {
  color: var(--text-muted);
  font-size: 13px;
}
.range-mode {
  height: 28px;
  padding: 0 6px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
  color: var(--text-primary);
  font-size: 12px;
  font-family: inherit;
}
.error {
  color: var(--danger);
  padding: 8px 10px;
  background: var(--danger-bg);
  border: 1px solid var(--danger-border);
  border-radius: var(--radius-pin);
  font-size: 13px;
}
.loading,
.empty {
  color: var(--text-muted);
  padding: 24px;
  text-align: center;
  font-size: 13px;
}
.chapter-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 360px;
  overflow: auto;
  padding: 4px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  background: var(--color-paper);
}
.row {
  display: grid;
  grid-template-columns: 24px 56px 1fr auto auto;
  align-items: center;
  gap: 10px;
  padding: 6px 10px;
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
  transition: background 0.1s;
}
.row.is-selected {
  background: var(--color-cinnabar-light);
}
.row.in-batch {
  opacity: 0.55;
  cursor: not-allowed;
}
.row .idx {
  color: var(--text-muted);
  font-family: var(--font-mono);
  font-size: 12px;
}
.row .title {
  color: var(--text-primary);
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row .words {
  color: var(--text-muted);
  font-family: var(--font-mono);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}
.badge {
  padding: 2px 8px;
  border-radius: var(--radius-pin);
  background: var(--bg-section);
  color: var(--text-muted);
  font-size: 11px;
}
</style>
