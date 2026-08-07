<template>
  <section>
    <PageHeader title="AI 调用" subtitle="每次 LLM chat 调用的元数据 / token / 预览 / 错误">
      <template #actions>
        <Button :disabled="loading" @click="reload()">刷新</Button>
        <Button kind="danger" :disabled="logs.length === 0" @click="onClear">清空</Button>
      </template>
    </PageHeader>

    <div class="toolbar">
      <label class="filter">
        <span>业务</span>
        <select :value="filter.business ?? ''" @change="onBusinessChange(($event.target as HTMLSelectElement).value)">
          <option value="">全部</option>
          <option value="transform_chapter">transform_chapter</option>
          <option value="test_model">test_model</option>
        </select>
      </label>
      <label class="filter">
        <span>状态</span>
        <select :value="filter.status ?? ''" @change="onStatusChange(($event.target as HTMLSelectElement).value)">
          <option value="">全部</option>
          <option value="success">成功</option>
          <option value="failed">失败</option>
        </select>
      </label>
      <label class="filter">
        <span>模型 id</span>
        <input
          type="number"
          min="1"
          :value="filter.model_config_id ?? ''"
          placeholder="(不限)"
          @change="onModelConfigChange(($event.target as HTMLInputElement).value)"
        />
      </label>
      <span class="meta">共 {{ logs.length }} / 限 {{ filter.limit }}</span>
    </div>

    <div v-if="error" class="alert">{{ error }}</div>

    <div v-if="loading && logs.length === 0" class="empty">加载中...</div>
    <div v-else-if="logs.length === 0" class="empty">
      <p class="empty-title">还没有 AI 调用日志</p>
      <p class="empty-hint">
        数据来自 transformer 路径(章节转换)与 test_model 路径(模型测试连通性)。
        Phase 2 接入后会随业务调用自动填充。
      </p>
    </div>
    <Table v-else :columns="columns" :data="logs" :row-key="(row) => row.id">
      <template #cell-time="{ row }">
        <span class="time">{{ formatTime(row.created_at) }}</span>
      </template>
      <template #cell-business="{ row }">
        <Tag :kind="row.business === 'transform_chapter' ? 'info' : 'warn'">
          {{ row.business === 'transform_chapter' ? '章节转换' : '模型测试' }}
        </Tag>
      </template>
      <template #cell-model="{ row }">
        <div class="model-cell">
          <div class="model-name">{{ row.model_name }}</div>
          <div class="model-url">{{ row.base_url }}</div>
        </div>
      </template>
      <template #cell-status="{ row }">
        <Tag :kind="row.status === 'success' ? 'success' : 'danger'">
          {{ row.status === 'success' ? '成功' : '失败' }}
        </Tag>
      </template>
      <template #cell-tokens="{ row }">
        <div class="tokens">
          <span>in 粗估 {{ row.estimated_tokens_in ?? '—' }}</span>
          <span class="arrow">→</span>
          <span :class="{ missing: row.actual_tokens_in === null }">
            实际 {{ row.actual_tokens_in ?? '—' }}
          </span>
          <span class="divider">/</span>
          <span>out {{ row.actual_tokens_out ?? '—' }}</span>
        </div>
      </template>
      <template #cell-latency="{ row }">{{ formatLatency(row.latency_ms) }}</template>
      <template #cell-error="{ row }">
        <span v-if="row.error" class="err-text" :title="row.error">
          {{ truncate(row.error, 40) }}
        </span>
        <span v-else class="muted">—</span>
      </template>
      <template #cell-actions="{ row }">
        <Button size="small" @click="openDetail(row.id)">详情</Button>
      </template>
    </Table>

    <AiCallDetail
      v-if="detailId !== null"
      :id="detailId"
      @close="detailId = null"
    />

    <ConfirmDialog
      v-model:open="clearConfirmOpen"
      title="清空 AI 调用日志"
      message="将删除全部 ai_call_logs 行(无法恢复)。transform / test_model 历史 token / 错误信息都会清空。确认?"
      kind="danger"
      confirm-text="清空"
      @confirm="doClear"
    />
  </section>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue';
import Button from '../components/ui/Button.vue';
import Table from '../components/ui/Table.vue';
import Tag from '../components/ui/Tag.vue';
import PageHeader from '../components/ui/PageHeader.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import AiCallDetail from '../components/AiCallDetail.vue';
import {
  clearAiCallLogs,
  listAiCallLogs,
} from '../ipc/commands';
import type { AiCallLog, AiCallLogFilter } from '../ipc/types';

const logs = ref<AiCallLog[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const detailId = ref<number | null>(null);
const clearConfirmOpen = ref(false);

/// 过滤状态 —— 任何字段改都触发 reload。
const filter = reactive<{
  business: AiCallLogFilter["business"];
  status: AiCallLogFilter["status"];
  model_config_id: AiCallLogFilter["model_config_id"];
  limit: number;
}>({
  business: null,
  status: null,
  model_config_id: null,
  limit: 200,
});

const columns = [
  { key: 'time', title: '时间', width: '170px' },
  { key: 'business', title: '业务', width: '120px' },
  { key: 'model', title: '模型', width: '240px' },
  { key: 'status', title: '状态', width: '80px' },
  { key: 'tokens', title: 'tokens (估 → 实 in / 实 out)', width: '260px' },
  { key: 'latency', title: '延迟', width: '90px' },
  { key: 'error', title: '错误' },
  { key: 'actions', title: '操作', width: '90px', type: 'actions' as const },
];

async function reload() {
  loading.value = true;
  error.value = null;
  try {
    logs.value = await listAiCallLogs({
      business: filter.business,
      status: filter.status,
      model_config_id: filter.model_config_id,
      limit: filter.limit,
    });
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

function onBusinessChange(v: string) {
  filter.business = v === '' ? null : (v as 'transform_chapter' | 'test_model');
  void reload();
}

function onStatusChange(v: string) {
  filter.status = v === '' ? null : (v as 'success' | 'failed');
  void reload();
}

function onModelConfigChange(v: string) {
  const n = v.trim() === '' ? null : Number(v);
  filter.model_config_id = n !== null && Number.isFinite(n) && n > 0 ? n : null;
  void reload();
}

function openDetail(id: number) {
  detailId.value = id;
}

function onClear() {
  clearConfirmOpen.value = true;
}

async function doClear() {
  try {
    await clearAiCallLogs();
    await reload();
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}

function formatTime(s: string): string {
  return s.replace('T', ' ').replace(/\.\d+/, '').replace('Z', '');
}

function formatLatency(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

function truncate(s: string, n: number): string {
  return s.length > n ? s.slice(0, n) + '…' : s;
}

onMounted(() => void reload());
</script>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 12px;
  flex-wrap: wrap;
}
.filter {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text-secondary);
}
.filter select,
.filter input {
  height: 28px;
  padding: 0 8px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
  color: var(--text-primary);
  font-size: 13px;
  font-family: inherit;
  outline: none;
}
.filter input { width: 90px; }
.meta {
  margin-left: auto;
  font-size: 12px;
  color: var(--text-muted);
}
.alert {
  padding: 12px 16px;
  background: var(--danger-bg);
  color: var(--danger);
  border-radius: var(--radius-pin);
  margin-bottom: 16px;
  border: 1px solid var(--danger-border);
}
.empty {
  text-align: center;
  padding: 48px 24px;
  color: var(--text-muted);
  border: 1px dashed var(--border-rouge);
  border-radius: var(--radius-card);
  background: var(--color-sheet);
}
.empty-title {
  font-size: 16px;
  color: var(--text-primary);
  margin: 0 0 8px;
}
.empty-hint {
  font-size: 13px;
  margin: 0;
  line-height: 1.6;
}
.time {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
  color: var(--text-secondary);
}
.model-cell {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.model-name {
  font-size: 13px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.model-url {
  font-size: 11px;
  color: var(--text-muted);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tokens {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}
.tokens .arrow { color: var(--text-muted); }
.tokens .divider { color: var(--text-muted); margin: 0 2px; }
.tokens .missing { color: var(--text-muted); font-style: italic; }
.err-text {
  color: var(--danger);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
}
.muted {
  color: var(--text-muted);
  font-size: 12px;
}
</style>