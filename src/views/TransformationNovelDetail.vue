<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onUnmounted, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useWorkflowsStore } from '../stores/workflows';
import {
  getChapter as ipcGetChapter,
  getTransformationNovel,
  listDataAssets,
} from '../ipc/commands';
import type {
  SourceChapterRow, WorkflowSummary, WorkflowChapterRow, ChapterWorkflowResultRow,
  CreateWorkflowInput, Chapter, TransformationNovelSummary, DataAssetRow,
} from '../ipc/types';
import Button from '../components/ui/Button.vue';
import PageHeader from '../components/ui/PageHeader.vue';
import IconArrowLeft from '~icons/lucide/arrow-left';
import { countWords, formatTime, formatWordCount } from '../utils/format';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import CreateBatchDialog from '../components/CreateBatchDialog.vue';
import PromoteWorkflowDialog from '../components/PromoteWorkflowDialog.vue';
import Dialog from '../components/ui/Dialog.vue';
import DataTable from '../components/ui/DataTable.vue';
import Tag from '../components/ui/Tag.vue';

const route = useRoute();
const router = useRouter();
const tnId = computed(() => Number(route.params.tnId));
/// 当前 tn 摘要:onMounted 里拉一次,详情页生命周期内不会变(没编辑入口)。
/// 拉不到时不静默 fallback,直接报错让用户看到。
const tnSummary = ref<TransformationNovelSummary | null>(null);
const tnError = ref<string | null>(null);
const tnTitle = computed(() => tnSummary.value?.title ?? '');
/// 源数据资产(从 listDataAssets 里按 tnSummary.data_asset_id 查)给 meta-strip 用。
const sourceAssets = ref<DataAssetRow[]>([]);
const sourceAsset = computed<DataAssetRow | null>(() => {
  if (tnSummary.value === null) return null;
  return sourceAssets.value.find((a) => a.id === tnSummary.value!.data_asset_id) ?? null;
});
async function loadTn() {
  tnError.value = null;
  try {
    const [tn, assets] = await Promise.all([
      getTransformationNovel(tnId.value),
      listDataAssets(),
    ]);
    tnSummary.value = tn;
    sourceAssets.value = assets;
  } catch (e: unknown) {
    tnError.value = e instanceof Error ? e.message : String(e);
  }
}

const store = useWorkflowsStore();

const activeTab = ref<'chapters' | 'workflows'>('chapters');

/// 章节来源 tab 表格自适应高度:监听 `main.app` 的尺寸变化,实时算出
/// `main 可用高度 - 表格 div 顶部偏移 - 底部留白`,下限 300px。
/// 表格内部垂直滚动仍然由 DataTable 的 maxHeight 触发,与此无关。
const chaptersTableEl = ref<HTMLElement | null>(null);
const chaptersTableMaxHeight = ref('420px');
const CHAPTERS_TABLE_MIN_HEIGHT = 300;
const CHAPTERS_TABLE_BOTTOM_PADDING = 48;
/// 调试时实测:横向滚动条(~17px) + 表格 border/padding/box-shadow(~10px)
/// 加起来约 26~36px。从 16 提到 36 给一个更稳的容错,避免 main.app
/// 因为表格占用溢出而出现滚动条。

function recalcChaptersTableHeight() {
  const main = document.querySelector('main.app') as HTMLElement | null;
  const tableEl = chaptersTableEl.value;
  if (main === null || tableEl === null) return;
  const mainRect = main.getBoundingClientRect();
  const tableRect = tableEl.getBoundingClientRect();
  const available = main.clientHeight - (tableRect.top - mainRect.top);
  chaptersTableMaxHeight.value = `${Math.max(CHAPTERS_TABLE_MIN_HEIGHT, available - CHAPTERS_TABLE_BOTTOM_PADDING)}px`;
}

/// 章节来源 tab 表格列(TanStack format)
const sourceColumns = [
  { id: 'pick', header: '', enableSorting: false },
  { accessorKey: 'idx', id: 'idx', header: '#', enableSorting: true },
  { accessorKey: 'title', header: '标题', enableSorting: true },
  { accessorKey: 'word_count', id: 'words', header: '字数', enableSorting: true },
  { accessorKey: 'non_empty_result_count', id: 'result_count', header: '已有结果数', enableSorting: true },
];
const sourceWidths: Record<string, number> = {
  pick: 40,
  idx: 60,
  title: 280,
  words: 100,
  result_count: 120,
};

/// 工作流 tab 表格列
const workflowColumns = [
  { accessorKey: 'label', header: '标签', enableSorting: true },
  { id: 'status', header: '状态', enableSorting: true },
  { accessorKey: 'total_count', id: 'total', header: '总章数', enableSorting: true },
  { accessorKey: 'done_count', id: 'done', header: '已完成', enableSorting: true },
  { accessorKey: 'failed_count', id: 'failed', header: '失败', enableSorting: true },
  { accessorKey: 'skipped_count', id: 'skipped', header: '已跳过', enableSorting: true },
  { accessorKey: 'created_at', id: 'created', header: '创建时间', enableSorting: true },
  { accessorKey: 'ended_at', id: 'ended', header: '结束时间', enableSorting: true },
  { id: 'actions', header: '操作', enableSorting: false },
];
const workflowWidths: Record<string, number> = {
  label: 180,
  status: 100,
  total: 80,
  done: 80,
  failed: 80,
  skipped: 80,
  created: 160,
  ended: 160,
  actions: 140,
};

/// 工作流详情表格列
const workflowChapterColumns = [
  { id: 'pick', header: '', enableSorting: false },
  { accessorKey: 'chapter_title', id: 'title', header: '标题', enableSorting: true },
  { id: 'status', header: '状态', enableSorting: true },
  { accessorKey: 'content_preview', id: 'preview', header: '结果预览', enableSorting: false },
  { accessorKey: 'error', header: '错误', enableSorting: false },
  { id: 'actions', header: '操作', enableSorting: false },
];
const workflowChapterWidths: Record<string, number> = {
  pick: 40,
  title: 200,
  status: 100,
  preview: 240,
  error: 200,
  actions: 200,
};

// 章节来源 tab
const selectedChapterIds = ref<Set<number>>(new Set());
const openSourceChapterId = ref<number | null>(null);

const sources = computed<SourceChapterRow[]>(() => store.sourcesByTn.get(tnId.value) ?? []);
const selectedCount = computed(() => selectedChapterIds.value.size);

function toggleSelect(chapterId: number, on: boolean) {
  const next = new Set(selectedChapterIds.value);
  if (on) next.add(chapterId); else next.delete(chapterId);
  selectedChapterIds.value = next;
}

/// 自定义范围选择:按 idx(#列序号)对一段章节做"覆盖"或"累加/取反"。
/// 用户视角就是 "#N 到 #M",内部映射回 chapter_id 写回 selectedChapterIds。
/// - replace:把选中集合直接替换为范围内的章节(经典覆盖)。
/// - toggle  :把范围内的每个 chapter 在当前选中集合中取反,范围外的不变
///            → 多次应用可累加:先 1~100,再 200~300,得到 1~300;再 50~150,
///            得到 1~49 + 151~300。这是用户实际选长篇小说的常用模式。
type RangeMode = 'replace' | 'toggle';
const rangeMode = ref<RangeMode>('toggle');
const rangeFrom = ref<number | null>(null);
const rangeTo = ref<number | null>(null);
/// 输入合法性:不能 < 1,不能 > list.length,任一不满足给错误样式并禁用"应用"。
const rangeError = computed<string | null>(() => {
  const list = sources.value;
  if (list.length === 0) return null;
  if (rangeFrom.value === null || rangeTo.value === null) return null;
  if (rangeFrom.value < 1 || rangeTo.value < 1) return '序号需 ≥ 1';
  if (rangeFrom.value > list.length || rangeTo.value > list.length) {
    return `序号需 ≤ ${list.length}`;
  }
  return null;
});
function applyRange() {
  const list = sources.value;
  if (list.length === 0) return;
  if (rangeError.value !== null) return;
  if (rangeFrom.value === null || rangeTo.value === null) return;
  // to < from 自动交换,idx 单调。
  const lo = Math.min(rangeFrom.value, rangeTo.value);
  const hi = Math.max(rangeFrom.value, rangeTo.value);
  const targetIds = new Set<number>(
    list.filter((s) => s.idx >= lo && s.idx <= hi).map((s) => s.chapter_id),
  );
  if (rangeMode.value === 'replace') {
    selectedChapterIds.value = targetIds;
  } else {
    // toggle:范围内每个 id 在当前集合里 add/delete 翻转,范围外不动。
    const next = new Set(selectedChapterIds.value);
    for (const id of targetIds) {
      if (next.has(id)) next.delete(id); else next.add(id);
    }
    selectedChapterIds.value = next;
  }
}
function clearSelection() {
  selectedChapterIds.value = new Set();
}

function selectAll() {
  selectedChapterIds.value = new Set(sources.value.map((s) => s.chapter_id));
}
function selectNone() {
  selectedChapterIds.value = new Set();
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

// 章节Detail侧边栏(章节来源 tab 用)
const openSourceResults = computed<ChapterWorkflowResultRow[]>(() => {
  if (openSourceChapterId.value === null) return [];
  return store.resultsByTnChapter.get(`${tnId.value}:${openSourceChapterId.value}`) ?? [];
});

const stopConfirmOpen = ref(false);
const stopTargetId = ref<number | null>(null);
const retrySelectedIds = ref<Set<number>>(new Set());

// 工作流转正
const promoteOpen = ref(false);
const promoteSubmitting = ref(false);
const promoteError = ref<string | null>(null);

function openPromoteDialog() {
  if (selectedWorkflow.value === null) return;
  promoteError.value = null;
  promoteOpen.value = true;
}
async function confirmPromote(title: string) {
  const sw = selectedWorkflow.value;
  if (sw === null) return;
  promoteSubmitting.value = true;
  promoteError.value = null;
  try {
    await store.promote(sw.id, title);
    promoteOpen.value = false;
  } catch (e: unknown) {
    promoteError.value = e instanceof Error ? e.message : String(e);
  } finally {
    promoteSubmitting.value = false;
  }
}

// 章节Detail侧边栏的源原文
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

// Chapter Detail modal (within Workflow Detail modal)
const detailChapter = ref<WorkflowChapterRow | null>(null);
const detailLoading = ref(false);
const detailTransformed = ref<string | null>(null);
const detailTransformedStatus = ref<string | null>(null);
const detailSourceWordCount = computed<number>(() => sourceChapterDetail.value?.word_count ?? 0);
const detailTransformedWordCount = computed<number>(() => detailTransformed.value === null ? 0 : countWords(detailTransformed.value));
async function openChapterDetail(c: WorkflowChapterRow) {
  detailChapter.value = c;
  detailTransformed.value = null;
  detailTransformedStatus.value = null;
  sourceChapterDetail.value = null;
  sourceChapterText.value = '';
  detailLoading.value = true;
  sourceChapterLoading.value = true;
  try {
    await Promise.all([
      store.loadResultsForChapter(tnId.value, c.chapter_id),
      ipcGetChapter(c.chapter_id).then((ch) => {
        sourceChapterDetail.value = ch;
        sourceChapterText.value = ch.body;
      }),
    ]);
    const list = store.resultsByTnChapter.get(`${tnId.value}:${c.chapter_id}`) ?? [];
    const match = list.find((r) => r.batch_id === selectedWorkflowId.value);
    detailTransformed.value = match?.content ?? null;
    detailTransformedStatus.value = match?.status ?? null;
  } catch (e: unknown) {
    console.error(e);
  } finally {
    detailLoading.value = false;
    sourceChapterLoading.value = false;
  }
}
function closeChapterDetail() {
  detailChapter.value = null;
  detailTransformed.value = null;
  detailTransformedStatus.value = null;
}
async function retryFromDetail() {
  const c = detailChapter.value;
  if (c === null || selectedWorkflowId.value === null) return;
  retrySubmitting.value = true;
  try {
    await store.retry(selectedWorkflowId.value, [c.chapter_id]);
    await store.loadChapters(selectedWorkflowId.value);
    closeChapterDetail();
  } catch (e: unknown) {
    console.error(e);
  } finally {
    retrySubmitting.value = false;
  }
}

const reconvertError = ref<string | null>(null);
async function reconvertSingle(c: WorkflowChapterRow) {
  if (selectedWorkflowId.value === null) return;
  if (c.status === 'running' || c.status === 'pending') {
    reconvertError.value = '该章节正在处理中，暂不可重新转换。';
    return;
  }
  reconvertError.value = null;
  try {
    await store.retry(selectedWorkflowId.value, [c.chapter_id]);
    await store.loadChapters(selectedWorkflowId.value);
  } catch (e: unknown) {
    reconvertError.value = e instanceof Error ? e.message : String(e);
  }
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

function canRetryChapter(c: WorkflowChapterRow): boolean {
  return c.status === 'failed' || c.status === 'skipped';
}

const retrySubmitting = ref(false);
const POLLABLE_STATUSES = new Set(['running']);
const isBatchLive = computed<boolean>(() => {
  const s = selectedWorkflow.value;
  return s !== null && POLLABLE_STATUSES.has(s.status);
});
const canRetrySelection = computed<boolean>(() => {
  const s = selectedWorkflow.value;
  if (s === null) return false;
  if (s.status !== 'stopped') return false;
  return retrySelectedIds.value.size > 0;
});
let chapterPollHandle: number | null = null;
function startChapterPoll() {
  if (chapterPollHandle !== null) return;
  chapterPollHandle = window.setInterval(() => {
    if (selectedWorkflowId.value === null) return;
    void store.loadChapters(selectedWorkflowId.value);
  }, 2000);
}
function stopChapterPoll() {
  if (chapterPollHandle !== null) {
    window.clearInterval(chapterPollHandle);
    chapterPollHandle = null;
  }
}
watch(selectedWorkflowId, (id) => {
  if (id !== null) startChapterPoll(); else stopChapterPoll();
}, { immediate: true });
watch(isBatchLive, (live) => {
  if (live) startChapterPoll(); else stopChapterPoll();
});
onUnmounted(() => stopChapterPoll());

async function doRetry() {
  if (selectedWorkflowId.value === null) return;
  const chapterIds = selectedWorkflowChapters.value
    .filter((c) => retrySelectedIds.value.has(c.tc_id))
    .map((c) => c.chapter_id);
  if (chapterIds.length === 0) return;
  retrySubmitting.value = true;
  try {
    await store.retry(selectedWorkflowId.value, chapterIds);
    retrySelectedIds.value = new Set();
    await store.loadChapters(selectedWorkflowId.value);
  } catch (e: unknown) {
    console.error(e);
  } finally {
    retrySubmitting.value = false;
  }
}

function fmtTime(s: string | null | undefined): string {
  return formatTime(s);
}
function formatWorkflowStatus(s: string): string {
  switch (s) {
    case 'pending': return '待处理';
    case 'running': return '转换中';
    case 'done': return '已完成';
    case 'failed': return '失败';
    case 'skipped': return '已跳过';
    case 'cancelled': return '已取消';
    case 'stopped': return '已停止';
    default: return s;
  }
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
  createBatchError.value = null;
}

const createBatchError = ref<string | null>(null);

async function onCreateBatch(payload: CreateWorkflowInput) {
  try {
    const w = await store.create(payload);
    selectedChapterIds.value = new Set();
    activeTab.value = 'workflows';
    await openWorkflowPanel(w);
    createBatchError.value = null;
  } catch (e: unknown) {
    createBatchError.value = e instanceof Error ? e.message : String(e);
  } finally {
    createBatchOpen.value = false;
  }
}

async function loadAll() {
  await Promise.all([
    store.loadSources(tnId.value),
    store.loadByTn(tnId.value),
    loadTn(),
  ]);
}

/// main.app 尺寸变化 → 重算章节来源表格高度。
let mainResizeObserver: ResizeObserver | null = null;

/// 章节来源表格表头全选/全不选。
function onToggleAll(e: Event) {
  const checked = (e.target as HTMLInputElement).checked;
  if (checked) selectAll();
  else selectNone();
}

/// 工作流章节:只勾选"可重试"且"空槽位"的行,这样按钮才有效。
const retryableCount = computed<number>(() =>
  selectedWorkflowChapters.value.filter((c) => canRetryChapter(c) && c.is_empty_slot).length,
);
function onToggleAllRetry(e: Event) {
  const checked = (e.target as HTMLInputElement).checked;
  const next = new Set<number>();
  if (checked) {
    for (const c of selectedWorkflowChapters.value) {
      if (canRetryChapter(c) && c.is_empty_slot) next.add(c.tc_id);
    }
  }
  retrySelectedIds.value = next;
}

let pollHandle: number | null = null;

onMounted(async () => {
  await loadAll();
  pollHandle = window.setInterval(() => { void store.loadByTn(tnId.value); }, 5000);
  // 等 sources 首次渲染到 DOM,再算一次;之后由 ResizeObserver 跟踪 main 尺寸变化。
  await nextTick();
  recalcChaptersTableHeight();
  const main = document.querySelector('main.app');
  if (main !== null) {
    mainResizeObserver = new ResizeObserver(() => recalcChaptersTableHeight());
    mainResizeObserver.observe(main);
  }
});

onUnmounted(() => {
  if (pollHandle !== null) window.clearInterval(pollHandle);
  if (mainResizeObserver !== null) {
    mainResizeObserver.disconnect();
    mainResizeObserver = null;
  }
});

/// sources 数量变化(数据加载完 / 切 tab 后重新显示)也会影响表格 div 在 main 内的位置,
/// 等 DOM 更新后重算一次。
watch([() => sources.value.length, activeTab], () => {
  void nextTick(() => recalcChaptersTableHeight());
});

watch(() => workflows.value, (list) => {
  if (selectedWorkflowId.value === null) return;
  if (!list.find((w) => w.id === selectedWorkflowId.value)) {
    selectedWorkflowId.value = null;
  }
});

// spec §9.1: 默认Select All。sources 第一次加载后初始化 selectedChapterIds,
// 后续用户手动Actions不会被覆盖。
let didInitSelection = false;
watch(() => sources.value, (list) => {
  if (didInitSelection || list.length === 0) return;
  selectedChapterIds.value = new Set(list.map((s) => s.chapter_id));
  didInitSelection = true;
}, { immediate: true });
</script>

<template>
  <section class="tn-detail">
    <PageHeader :title="tnTitle || '加载中...'" size="small">
      <template #back>
        <Button aria-label="返回" @click="router.back()">
          <IconArrowLeft :size="16" :stroke-width="1.5" />
        </Button>
      </template>
      <template #actions>
        <!-- 占位:未来若加"重命名"等动作放这里 -->
      </template>
    </PageHeader>

    <div v-if="tnError" class="alert">{{ tnError }}</div>

    <div v-if="tnSummary" class="meta-strip">
      <div class="tags">
        <Tag>转换工程</Tag>
        <span v-if="sourceAsset" class="badge">来自「{{ sourceAsset.title }}」</span>
      </div>
      <div class="meta-text">
        <span>{{ formatTime(tnSummary.created_at) }}</span>
        <span v-if="tnSummary.chapters_count > 0" class="src">共 {{ tnSummary.chapters_count }} 章</span>
        <span v-if="tnSummary.note" class="src" :title="tnSummary.note">备注:{{ tnSummary.note }}</span>
      </div>
    </div>

    <div class="tabs">
      <button :class="{ active: activeTab === 'chapters' }" @click="activeTab = 'chapters'">
        章节来源
      </button>
      <button :class="{ active: activeTab === 'workflows' }" @click="activeTab = 'workflows'">
        工作流
      </button>
    </div>

    <!-- 章节来源 tab -->
    <template v-if="activeTab === 'chapters'">
      <div class="actions">
        <div class="range-pick">
          <span class="range-label">按 # 选</span>
          <input
            type="number"
            class="range-input"
            :class="{ 'has-error': rangeError !== null }"
            v-model.number="rangeFrom"
            :min="1"
            :max="sources.length"
            placeholder="起"
            :disabled="sources.length === 0"
            @keydown.enter="applyRange"
          />
          <span class="range-sep">—</span>
          <input
            type="number"
            class="range-input"
            :class="{ 'has-error': rangeError !== null }"
            v-model.number="rangeTo"
            :min="1"
            :max="sources.length"
            placeholder="止"
            :disabled="sources.length === 0"
            @keydown.enter="applyRange"
          />
          <select
            v-model="rangeMode"
            class="range-mode"
            :disabled="sources.length === 0"
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
          >
            应用
          </Button>
          <Button
            size="small"
            :disabled="selectedCount === 0"
            @click="clearSelection"
          >
            清空
          </Button>
        </div>
        <Button
          kind="primary"
          size="small"
          :disabled="selectedCount === 0"
          @click="openCreateBatch"
        >
          新建工作流 ({{ selectedCount }} 章）
        </Button>
      </div>
      <div ref="chaptersTableEl" class="chapters-table-wrap">
        <DataTable
          v-if="sources.length > 0"
          :columns="sourceColumns"
          :data="sources"
          :row-key="(row: SourceChapterRow) => row.chapter_id"
          :widths="sourceWidths"
          :numeric-columns="['idx', 'words', 'result_count']"
          :max-height="chaptersTableMaxHeight"
          empty-text="暂无章节"
        >
        <template #header-pick>
          <input
            type="checkbox"
            :checked="selectedChapterIds.size === sources.length && sources.length > 0"
            :indeterminate.prop="selectedChapterIds.size > 0 && selectedChapterIds.size < sources.length"
            aria-label="全选章节"
            @click.stop
            @change="onToggleAll($event)"
          />
        </template>
        <template #cell-pick="{ row }">
          <input
            type="checkbox"
            :checked="selectedChapterIds.has(row.chapter_id)"
            @change="(e) => toggleSelect(row.chapter_id, (e.target as HTMLInputElement).checked)"
          />
        </template>
        <template #cell-title="{ row }">
          <button class="link-btn" @click="openChapterPanel(row.chapter_id)">{{ row.title }}</button>
        </template>
        </DataTable>
        <div v-else class="empty">暂无章节</div>
      </div>
    </template>

    <!-- 工作流 tab -->
    <template v-else>
      <DataTable
        v-if="workflows.length > 0"
        :columns="workflowColumns"
        :data="workflows"
        :row-key="(row: WorkflowSummary) => row.id"
        :widths="workflowWidths"
        :numeric-columns="['total', 'done', 'failed', 'skipped']"
        frozen-column="actions"
        empty-text="尚无工作流"
      >
        <template #cell-status="{ row }">
          <span class="status" :class="row.status">{{ formatWorkflowStatus(row.status) }}</span>
          <span
            v-if="row.promoted_count > 0"
            class="promoted-tag"
            :title="`已基于此工作流转正 ${row.promoted_count} 份数据资产`"
          >转正 × {{ row.promoted_count }}</span>
        </template>
        <template #cell-created="{ row }">
          {{ fmtTime(row.created_at) }}
        </template>
        <template #cell-ended="{ row }">
          {{ fmtTime(row.ended_at) }}
        </template>
        <template #cell-actions="{ row }">
          <button type="button" class="row-link" @click="openWorkflowPanel(row)">详情</button>
        </template>
      </DataTable>
      <div v-if="createBatchError" class="error-banner">
        <span>新建工作流失败：{{ createBatchError }}</span>
        <button type="button" class="dismiss" aria-label="关闭" @click="createBatchError = null">×</button>
      </div>
      <div v-if="workflows.length === 0 && !createBatchError" class="empty">暂无工作流</div>
    </template>

    <!-- 章节Detail侧边面板 -->
    <div v-if="openSourceChapterId !== null" class="side-panel">
      <div class="panel-header">
        <h3>Chapter #{{ openSourceChapterId }} 的工作流结果</h3>
        <Button size="small" @click="closeChapterPanel">关闭</Button>
      </div>
      <section class="original-section">
        <h4>源原文</h4>
        <div v-if="sourceChapterLoading" class="hint">加载中...</div>
        <pre v-else-if="sourceChapterText" class="result-content">{{ sourceChapterText }}</pre>
        <div v-else class="hint">暂无原文</div>
      </section>
      <section class="results-section">
        <h4>本章节的转换结果</h4>
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

    <!-- Workflow Detail侧边面板 -->
    <Dialog v-if="selectedWorkflow !== null" :open="true" title="工作流详情" :width="1100" @update:open="closeWorkflowPanel">
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
        <Button
          v-if="canRetrySelection"
          kind="primary"
          size="small"
          :disabled="retrySelectedIds.size === 0 || retrySubmitting"
          :loading="retrySubmitting"
          @click="doRetry"
        >
          ↻ 重试所选 ({{ retrySelectedIds.size }})
        </Button>
        <Button
          v-if="selectedWorkflow.status === 'stopped'"
          size="small"
          :loading="promoteSubmitting"
          @click="openPromoteDialog"
        >
          ▶ 转为数据资产
        </Button>
        <span
          v-if="selectedWorkflow.status === 'stopped' && selectedWorkflow.promoted_count > 0"
          class="promoted-tag"
          :title="`已基于此工作流转正 ${selectedWorkflow.promoted_count} 份数据资产`"
        >
          已转正 × {{ selectedWorkflow.promoted_count }}
        </span>
      </div>
      <div v-if="reconvertError" class="error-banner">
        <span>重新转换失败：{{ reconvertError }}</span>
        <button type="button" class="dismiss" aria-label="关闭" @click="reconvertError = null">×</button>
      </div>
      <DataTable
        v-if="selectedWorkflowChapters.length > 0"
        :columns="workflowChapterColumns"
        :data="selectedWorkflowChapters"
        :row-key="(row: WorkflowChapterRow) => row.tc_id"
        :widths="workflowChapterWidths"
        :truncate-columns="['title', 'preview', 'error']"
        frozen-column="actions"
        empty-text="暂无章节"
      >
        <template #header-pick>
          <input
            type="checkbox"
            :checked="retrySelectedIds.size === retryableCount && retryableCount > 0"
            :indeterminate.prop="retrySelectedIds.size > 0 && retrySelectedIds.size < retryableCount"
            aria-label="全选可重试章节"
            :disabled="retryableCount === 0"
            @click.stop
            @change="onToggleAllRetry($event)"
          />
        </template>
        <template #cell-pick="{ row }">
          <input
            v-if="canRetryChapter(row)"
            type="checkbox"
            :disabled="!row.is_empty_slot"
            :checked="retrySelectedIds.has(row.tc_id)"
            @change="(e) => toggleRetrySelection(row.tc_id, (e.target as HTMLInputElement).checked)"
          />
        </template>
        <template #cell-status="{ row }">
          <span v-if="row.status === 'running'" class="dot dot-running" />
          <span v-else-if="row.status === 'pending'" class="dot dot-pending" />
          <span class="status" :class="row.status">{{ formatWorkflowStatus(row.status) }}</span>
        </template>
        <template #cell-actions="{ row }">
          <button type="button" class="row-link" @click="openChapterDetail(row)">详情</button>
          <span class="row-sep" aria-hidden="true">·</span>
          <button type="button" class="row-link" @click="reconvertSingle(row)">重新转换</button>
        </template>
      </DataTable>
      <div v-else class="empty">暂无章节</div>
    </Dialog>

    <!-- Chapter Detail modal (within Workflow Detail) -->
    <Dialog v-if="detailChapter !== null" :open="true" title="章节详情" :width="1200" @update:open="closeChapterDetail">
      <div class="detail-grid">
        <section>
          <h4>
            Source Original
            <span v-if="!sourceChapterLoading && sourceChapterText" class="word-count">{{ formatWordCount(detailSourceWordCount) }}</span>
          </h4>
          <div v-if="sourceChapterLoading" class="hint">Loading...</div>
          <pre v-else-if="sourceChapterText" class="result-content">{{ sourceChapterText }}</pre>
          <div v-else class="hint">No source text</div>
        </section>
        <section>
          <h4>
            Transformed
            <span v-if="detailTransformedStatus" class="status" :class="detailTransformedStatus">{{ detailTransformedStatus }}</span>
            <span v-if="!detailLoading && detailTransformed" class="word-count">{{ formatWordCount(detailTransformedWordCount) }}</span>
          </h4>
          <div v-if="detailLoading" class="hint">Loading...</div>
          <pre v-else-if="detailTransformed" class="result-content">{{ detailTransformed }}</pre>
          <div v-else class="hint">尚未转换</div>
        </section>
      </div>
      <template #footer>
        <Button
          v-if="detailChapter !== null && canRetryChapter(detailChapter)"
          kind="primary"
          size="small"
          :disabled="!detailChapter.is_empty_slot || retrySubmitting"
          :loading="retrySubmitting"
          @click="retryFromDetail"
        >重试</Button>
        <Button size="small" @click="closeChapterDetail">关闭</Button>
      </template>
    </Dialog>


    <PromoteWorkflowDialog
      v-if="selectedWorkflow !== null"
      v-model:open="promoteOpen"
      :workflow-label="selectedWorkflow.label ?? `工作流 #${selectedWorkflow.id}`"
      :source-data-asset-title="tnTitle"
      :success-count="selectedWorkflow.done_count"
      :fail-count="selectedWorkflow.failed_count"
      :skip-count="selectedWorkflow.skipped_count"
      @confirm="confirmPromote"
    />

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
      title="Stop Workflow"
      :message="'停止后当前章节会完成,后续章节标记为已跳过。确定停止吗?'"
      kind="danger"
      confirm-text="停止"
      @confirm="confirmStopWorkflow"
    />
  </section>
</template>

<style scoped>
.tn-detail { padding: 16px; }
.meta-strip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 0;
  margin-bottom: 8px;
}
.tags {
  display: flex;
  align-items: center;
  gap: 6px;
}
.badge {
  padding: 2px 8px;
  background: var(--bg-hover);
  border-radius: var(--radius-pin);
  font-size: 11px;
  color: var(--text-secondary);
}
.meta-text {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--text-secondary);
}
.meta-text .src {
  color: var(--text-secondary);
}
.alert {
  margin-top: 12px;
  padding: 8px 12px;
  background: var(--color-paper-mist);
  color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin);
  font-size: 13px;
}
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
.range-pick {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text-secondary);
}
.range-label {
  white-space: nowrap;
}
.range-sep {
  color: var(--text-muted);
}
.range-input {
  width: 64px;
  padding: 4px 8px;
  font: inherit;
  font-size: 13px;
  text-align: center;
  background: var(--color-sheet);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  color: var(--text-primary);
  outline: none;
}
.range-input:focus {
  border-color: var(--border-strong);
}
.range-input:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
.range-input.has-error {
  border-color: var(--danger, #d64545);
  background: var(--danger-bg, rgba(214, 69, 69, 0.06));
}
.range-mode {
  height: 28px;
  padding: 0 6px;
  font: inherit;
  font-size: 13px;
  background: var(--color-sheet);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  color: var(--text-primary);
  outline: none;
  cursor: pointer;
}
.range-mode:focus {
  border-color: var(--border-strong);
}
.range-mode:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
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
.chapter-row.running { background: rgba(196, 92, 60, 0.06); }
.detail-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; min-height: 400px; }
.detail-grid section { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
.detail-grid h4 { margin: 0; font-size: 14px; color: var(--text-secondary); display: flex; align-items: center; gap: 8px; }
.detail-grid h4 .word-count {
  font-size: 12px;
  color: var(--text-muted);
  font-weight: var(--font-weight-regular);
  font-family: var(--font-mono);
  margin-left: auto;
}
.detail-grid .result-content { background: var(--bg-section); border: 1px solid var(--border-soft); border-radius: var(--radius-pin); padding: 12px; white-space: pre-wrap; word-break: break-word; max-height: 60vh; overflow: auto; font-family: var(--font-mono); font-size: 13px; line-height: 1.6; }
.row-actions { text-align: right; }
.dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  margin-right: 6px;
  border-radius: 50%;
  vertical-align: middle;
}
.dot-running { background: var(--color-cinnabar); animation: pulse 1.2s ease-in-out infinite; }
.dot-pending { background: var(--text-muted); opacity: 0.55; }
.chapter-row.running .status.running { background: var(--color-cinnabar); color: #faf6ee; border-color: var(--color-cinnabar); }
.spinner-dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  margin-right: 6px;
  border-radius: 50%;
  background: var(--color-cinnabar);
  animation: pulse 1.2s ease-in-out infinite;
  vertical-align: middle;
}
@keyframes pulse { 0%, 100% { opacity: 0.35; } 50% { opacity: 1; } }

.error-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
  padding: 10px 12px;
  background: var(--danger-bg, rgba(214, 69, 69, 0.12));
  border: 1px solid var(--danger, #d64545);
  border-radius: var(--radius-pin, 6px);
  color: var(--danger, #d64545);
  font-size: 13px;
}
.error-banner .dismiss {
  background: transparent;
  border: none;
  color: inherit;
  font-size: 18px;
  line-height: 1;
  cursor: pointer;
  padding: 0 4px;
}

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
.promoted-tag {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 10px;
  background: #e8f5e9;
  color: #2e7d32;
  font-size: 12px;
  font-weight: 500;
  margin-right: 6px;
  cursor: default;
  white-space: nowrap;
}
</style>
