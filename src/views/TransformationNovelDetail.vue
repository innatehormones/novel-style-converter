<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useWorkflowsStore } from '../stores/workflows';
import { getChapter as ipcGetChapter } from '../ipc/commands';
import type {
  SourceChapterRow, WorkflowSummary, WorkflowChapterRow, ChapterWorkflowResultRow,
  CreateWorkflowInput, Chapter,
} from '../ipc/types';
import Button from '../components/ui/Button.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import CreateBatchDialog from '../components/CreateBatchDialog.vue';

const route = useRoute();
const router = useRouter();
const tnId = computed(() => Number(route.params.tnId));

const store = useWorkflowsStore();

const activeTab = ref<'chapters' | 'workflows'>('chapters');

// 章节一览 tab
const selectedChapterIds = ref<Set<number>>(new Set());
const openSourceChapterId = ref<number | null>(null);

const sources = computed<SourceChapterRow[]>(() => store.sourcesByTn.get(tnId.value) ?? []);
const selectedCount = computed(() => selectedChapterIds.value.size);

function toggleSelect(chapterId: number, on: boolean) {
  const next = new Set(selectedChapterIds.value);
  if (on) next.add(chapterId); else next.delete(chapterId);
  selectedChapterIds.value = next;
}

function selectAll() {
  selectedChapterIds.value = new Set(sources.value.map((s) => s.chapter_id));
}
function selectNone() {
  selectedChapterIds.value = new Set();
}
function invertSelection() {
  const next = new Set<number>();
  for (const s of sources.value) {
    if (!selectedChapterIds.value.has(s.chapter_id)) next.add(s.chapter_id);
  }
  selectedChapterIds.value = next;
}

// 工作流 tab
const workflows = computed<WorkflowSummary[]>(() => store.byTn.get(tnId.value) ?? []);
const selectedWorkflowId = ref<number | null>(null);
const selectedWorkflowChapters = computed<WorkflowChapterRow[]>(() =>
  selectedWorkflowId.value === null ? [] : (store.chaptersByBatch.get(selectedWorkflowId.value) ?? []),
);
const selectedWorkflow = computed<WorkflowSummary | null>(() => {
  if (selectedWorkflowId.value === null) return null;
  return workflows.value.find((w) => w.id === selectedWorkflowId.value) ?? null;
});

// 章节详情侧边栏(章节一览 tab 用)
const openSourceResults = computed<ChapterWorkflowResultRow[]>(() => {
  if (openSourceChapterId.value === null) return [];
  return store.resultsByTnChapter.get(`${tnId.value}:${openSourceChapterId.value}`) ?? [];
});

const stopConfirmOpen = ref(false);
const stopTargetId = ref<number | null>(null);
const retrySelectedIds = ref<Set<number>>(new Set());

// 章节详情侧边栏的源原文
const sourceChapterDetail = ref<Chapter | null>(null);
const sourceChapterText = ref<string>('');
const sourceChapterLoading = ref(false);

function toggleRetrySelection(tcId: number, on: boolean) {
  const next = new Set(retrySelectedIds.value);
  if (on) next.add(tcId); else next.delete(tcId);
  retrySelectedIds.value = next;
}

async function openChapterPanel(chapterId: number) {
  openSourceChapterId.value = chapterId;
  sourceChapterDetail.value = null;
  sourceChapterText.value = '';
  await store.loadResultsForChapter(tnId.value, chapterId);
  // 拉章节 + 切 data_asset 内容 → 显示源原文
  sourceChapterLoading.value = true;
  try {
    const ch = await ipcGetChapter(chapterId);
    sourceChapterDetail.value = ch;
    sourceChapterText.value = ch.body;
  } catch (e: unknown) {
    console.error(e);
  } finally {
    sourceChapterLoading.value = false;
  }
}

function closeChapterPanel() {
  openSourceChapterId.value = null;
  sourceChapterDetail.value = null;
  sourceChapterText.value = '';
}

async function openWorkflowPanel(w: WorkflowSummary) {
  selectedWorkflowId.value = w.id;
  retrySelectedIds.value = new Set();
  await store.loadChapters(w.id);
}

function closeWorkflowPanel() {
  selectedWorkflowId.value = null;
  retrySelectedIds.value = new Set();
}

function askStopWorkflow(id: number) {
  stopTargetId.value = id;
  stopConfirmOpen.value = true;
}

async function confirmStopWorkflow() {
  const id = stopTargetId.value;
  if (id === null) return;
  try {
    await store.stop(id);
    await store.loadChapters(id);
  } catch (e: unknown) {
    console.error(e);
  }
  stopTargetId.value = null;
}

async function doRetry() {
  if (selectedWorkflowId.value === null) return;
  const chapterIds = selectedWorkflowChapters.value
    .filter((c) => retrySelectedIds.value.has(c.tc_id))
    .map((c) => c.chapter_id);
  if (chapterIds.length === 0) return;
  try {
    await store.retry(selectedWorkflowId.value, chapterIds);
    retrySelectedIds.value = new Set();
    await store.loadChapters(selectedWorkflowId.value);
  } catch (e: unknown) {
    console.error(e);
  }
}

function fmtTime(s: string | null): string {
  if (s === null) return '—';
  return s;
}

// 新建工作流弹窗
const createBatchOpen = ref(false);
const createBatchDefaults = ref<{
  default_prompt_id: number | null;
  default_model_config_id: number | null;
  default_mode: 'compress' | 'style' | null;
}>({ default_prompt_id: null, default_model_config_id: null, default_mode: null });
// 当前 tn 的默认值(从最近一个 workflow / 自身 default 取),简化处理:从已有 workflow 继承。
function openCreateBatch() {
  // 简单复用最近一个 workflow 的 prompt/model 字段作默认,缺失时为 null。
  const recent = workflows.value[0];
  createBatchDefaults.value = {
    default_prompt_id: null,
    default_model_config_id: null,
    default_mode: null,
    ...(recent ? {
      // 这里没有 workflow row 本身的 prompt/model 字段(见 WorkflowSummary),
      // 所以保留 null,由用户挑选;prompt list 仍会按 default_mode 过滤。
    } : {}),
  };
  createBatchOpen.value = true;
}

async function onCreateBatch(payload: CreateWorkflowInput) {
  try {
    const w = await store.createAndRun(payload);
    createBatchOpen.value = false;
    selectedChapterIds.value = new Set();
    activeTab.value = 'workflows';
    await openWorkflowPanel(w);
  } catch (e: unknown) {
    console.error(e);
  }
}

async function loadAll() {
  await Promise.all([store.loadSources(tnId.value), store.loadByTn(tnId.value)]);
}

let pollHandle: number | null = null;

onMounted(async () => {
  await loadAll();
  pollHandle = window.setInterval(() => { void store.loadByTn(tnId.value); }, 5000);
});

onUnmounted(() => {
  if (pollHandle !== null) window.clearInterval(pollHandle);
});

watch(() => workflows.value, (list) => {
  if (selectedWorkflowId.value === null) return;
  if (!list.find((w) => w.id === selectedWorkflowId.value)) {
    selectedWorkflowId.value = null;
  }
});

// spec §9.1: 默认全选。sources 第一次加载后初始化 selectedChapterIds,
// 后续用户手动操作不会被覆盖。
let didInitSelection = false;
watch(() => sources.value, (list) => {
  if (didInitSelection || list.length === 0) return;
  selectedChapterIds.value = new Set(list.map((s) => s.chapter_id));
  didInitSelection = true;
}, { immediate: true });
</script>

<template>
  <section class="tn-detail">
    <header class="header">
      <h1>转换工程详情</h1>
      <p class="subtitle">TN #{{ tnId }}</p>
      <Button @click="router.back()">← 返回</Button>
    </header>

    <div class="tabs">
      <button :class="{ active: activeTab === 'chapters' }" @click="activeTab = 'chapters'">
        章节一览
      </button>
      <button :class="{ active: activeTab === 'workflows' }" @click="activeTab = 'workflows'">
        工作流
      </button>
    </div>

    <!-- 章节一览 tab -->
    <template v-if="activeTab === 'chapters'">
      <div class="actions">
        <Button size="small" @click="selectAll">全选</Button>
        <Button size="small" @click="selectNone">全不选</Button>
        <Button size="small" @click="invertSelection">反选</Button>
        <Button
          kind="primary"
          size="small"
          :disabled="selectedCount === 0"
          @click="openCreateBatch"
        >
          ▶ 新建工作流（{{ selectedCount }} 章）
        </Button>
      </div>
      <table v-if="sources.length > 0" class="chapter-table">
        <thead>
          <tr>
            <th style="width: 40px">勾选</th>
            <th style="width: 60px">#</th>
            <th>标题</th>
            <th style="width: 100px">字数</th>
            <th style="width: 120px">已有结果数</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="s in sources" :key="s.chapter_id">
            <td>
              <input
                type="checkbox"
                :checked="selectedChapterIds.has(s.chapter_id)"
                @change="(e) => toggleSelect(s.chapter_id, (e.target as HTMLInputElement).checked)"
              />
            </td>
            <td>{{ s.idx }}</td>
            <td>
              <button class="link-btn" @click="openChapterPanel(s.chapter_id)">{{ s.title }}</button>
            </td>
            <td>{{ s.word_count }}</td>
            <td>{{ s.non_empty_result_count }}</td>
          </tr>
        </tbody>
      </table>
      <div v-else class="empty">暂无章节</div>
    </template>

    <!-- 工作流 tab -->
    <template v-else>
      <table v-if="workflows.length > 0" class="batch-table">
        <thead>
          <tr>
            <th>标签</th>
            <th style="width: 100px">状态</th>
            <th style="width: 80px">总章节数</th>
            <th style="width: 80px">Done</th>
            <th style="width: 80px">Failed</th>
            <th style="width: 80px">Skipped</th>
            <th style="width: 160px">创建</th>
            <th style="width: 160px">结束</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="w in workflows" :key="w.id" @click="openWorkflowPanel(w)">
            <td>{{ w.label ?? '—' }}</td>
            <td>
              <span class="status" :class="w.status">{{ w.status }}</span>
            </td>
            <td>{{ w.total_count }}</td>
            <td>{{ w.done_count }}</td>
            <td>{{ w.failed_count }}</td>
            <td>{{ w.skipped_count }}</td>
            <td>{{ fmtTime(w.created_at) }}</td>
            <td>{{ fmtTime(w.ended_at) }}</td>
          </tr>
        </tbody>
      </table>
      <div v-else class="empty">暂无工作流</div>
    </template>

    <!-- 章节详情侧边面板 -->
    <div v-if="openSourceChapterId !== null" class="side-panel">
      <div class="panel-header">
        <h3>章节 #{{ openSourceChapterId }} 的工作流结果</h3>
        <Button size="small" @click="closeChapterPanel">关闭</Button>
      </div>
      <section class="original-section">
        <h4>源原文</h4>
        <div v-if="sourceChapterLoading" class="hint">加载中...</div>
        <pre v-else-if="sourceChapterText" class="result-content">{{ sourceChapterText }}</pre>
        <div v-else class="hint">暂无原文</div>
      </section>
      <section class="results-section">
        <h4>各工作流结果</h4>
        <div v-if="openSourceResults.length === 0" class="empty">暂无结果</div>
        <ul v-else class="result-list">
          <li v-for="r in openSourceResults" :key="r.batch_id" class="result-item">
            <div class="result-meta">
              <span>工作流 #{{ r.batch_id }} · {{ r.batch_label ?? '—' }}</span>
              <span class="status" :class="r.batch_status">{{ r.status }} / {{ r.batch_status }}</span>
            </div>
            <pre class="result-content">{{ r.content ?? '(空)' }}</pre>
          </li>
        </ul>
      </section>
    </div>

    <!-- 工作流详情侧边面板 -->
    <div v-if="selectedWorkflow !== null" class="side-panel">
      <div class="panel-header">
        <h3>
          工作流 #{{ selectedWorkflow.id }}{{ selectedWorkflow.label ? ' · ' + selectedWorkflow.label : '' }}
        </h3>
        <Button size="small" @click="closeWorkflowPanel">关闭</Button>
      </div>
      <div class="panel-actions">
        <Button
          v-if="selectedWorkflow.status === 'running'"
          kind="danger"
          size="small"
          @click="askStopWorkflow(selectedWorkflow.id)"
        >
          ⏹ 停止工作流
        </Button>
        <template v-else-if="selectedWorkflow.status === 'stopped'">
          <Button
            kind="primary"
            size="small"
            :disabled="retrySelectedIds.size === 0"
            @click="doRetry"
          >
            ↻ 重试所选（{{ retrySelectedIds.size }}）
          </Button>
        </template>
      </div>
      <table v-if="selectedWorkflowChapters.length > 0" class="chapter-table">
        <thead>
          <tr>
            <th v-if="selectedWorkflow.status === 'stopped'" style="width: 40px">勾选</th>
            <th style="width: 60px">#</th>
            <th>标题</th>
            <th style="width: 100px">状态</th>
            <th>结果预览</th>
            <th style="width: 200px">错误</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="c in selectedWorkflowChapters" :key="c.tc_id">
            <td v-if="selectedWorkflow.status === 'stopped'">
              <input
                type="checkbox"
                :disabled="!(c.status === 'failed' || c.status === 'skipped') || !c.is_empty_slot"
                :checked="retrySelectedIds.has(c.tc_id)"
                @change="(e) => toggleRetrySelection(c.tc_id, (e.target as HTMLInputElement).checked)"
              />
            </td>
            <td>{{ c.chapter_idx }}</td>
            <td>{{ c.chapter_title }}</td>
            <td>{{ c.status }}</td>
            <td class="preview">{{ c.content_preview ?? '—' }}</td>
            <td class="error">{{ c.error ?? '' }}</td>
          </tr>
        </tbody>
      </table>
      <div v-else class="empty">暂无章节</div>
    </div>

    <CreateBatchDialog
      v-model:open="createBatchOpen"
      :tn-id="tnId"
      :default-prompt-id="createBatchDefaults.default_prompt_id"
      :default-model-config-id="createBatchDefaults.default_model_config_id"
      :default-mode="createBatchDefaults.default_mode"
      :selected-chapter-ids="Array.from(selectedChapterIds)"
      @submit="onCreateBatch"
    />

    <ConfirmDialog
      v-model:open="stopConfirmOpen"
      title="停止工作流"
      :message="'停止后当前章节会完成,后续章节标记为 Skipped。确定停止吗?'"
      kind="danger"
      confirm-text="停止"
      @confirm="confirmStopWorkflow"
    />
  </section>
</template>

<style scoped>
.tn-detail { padding: 16px; }
.header { display: flex; align-items: center; gap: 12px; margin-bottom: 16px; }
.header h1 { margin: 0; font-size: 20px; }
.subtitle { margin: 0; color: var(--text-muted); }
.tabs { display: flex; gap: 8px; margin-bottom: 16px; border-bottom: 1px solid var(--border-color); }
.tabs button {
  padding: 8px 16px;
  background: transparent;
  border: none;
  border-bottom: 2px solid transparent;
  cursor: pointer;
  font-family: inherit;
  font-size: 14px;
  color: var(--text-secondary);
}
.tabs button.active {
  color: var(--color-primary);
  border-bottom-color: var(--color-primary);
}
.actions {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
  align-items: center;
}
.chapter-table, .batch-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 14px;
}
.chapter-table th, .chapter-table td,
.batch-table th, .batch-table td {
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-color);
  text-align: left;
  color: var(--text-primary);
}
.chapter-table th, .batch-table th {
  font-size: 12px;
  color: var(--text-muted);
  font-weight: var(--font-weight-regular);
}
.batch-table tbody tr { cursor: pointer; }
.batch-table tbody tr:hover td { background: var(--bg-hover); }
.link-btn {
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
  color: var(--color-primary);
  font: inherit;
  text-align: left;
}
.link-btn:hover { text-decoration: underline; }
.empty {
  text-align: center;
  padding: 48px 0;
  color: var(--text-muted);
  border: 1px dashed var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
}
.status {
  display: inline-block;
  padding: 1px 8px;
  border-radius: var(--radius-pin);
  font-size: 11px;
  border: 1px solid var(--border-soft);
}
.status.running { background: var(--bg-section); color: var(--text-secondary); }
.status.stopped { background: var(--warn-bg); color: var(--warn); border-color: var(--warn-border); }
.side-panel {
  position: fixed;
  top: 0;
  right: 0;
  width: 420px;
  height: 100vh;
  background: var(--color-sheet);
  border-left: 1px solid var(--border-color);
  padding: 16px;
  overflow: auto;
  box-shadow: -2px 0 12px rgba(0, 0, 0, 0.08);
  z-index: 10;
}
.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}
.panel-header h3 { margin: 0; font-size: 16px; }
.panel-actions {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
}
.preview {
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-secondary);
  font-size: 13px;
}
.error {
  color: var(--danger);
  font-size: 12px;
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
}
.result-list { list-style: none; padding: 0; margin: 0; }
.result-item {
  padding: 10px 0;
  border-bottom: 1px solid var(--border-soft);
}
.result-meta {
  display: flex;
  justify-content: space-between;
  font-size: 12px;
  color: var(--text-muted);
  margin-bottom: 6px;
}
.result-content {
  background: var(--bg-section);
  padding: 8px;
  border-radius: var(--radius-pin);
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 13px;
  font-family: var(--font-mono);
  margin: 0;
  max-height: 240px;
  overflow: auto;
}
.original-section, .results-section { margin-bottom: 16px; }
.original-section h4, .results-section h4 {
  margin: 0 0 8px;
  font-size: 13px;
  color: var(--text-muted);
  font-weight: var(--font-weight-regular);
}
.hint {
  font-size: 12px;
  color: var(--text-muted);
  padding: 8px 0;
}
</style>
