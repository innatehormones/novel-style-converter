<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { listTransformationChapters } from '../ipc/commands';
import { useBatchesStore } from '../stores/batches';
import type { TransformationChapterRow } from '../ipc/types';
// Task 10 删除本文件前,Batch 类型从旧 store shim 借。
import type { Batch } from '../stores/batches';

const route = useRoute();
const router = useRouter();
const tnId = computed(() => Number(route.params.tnId));

const batchesStore = useBatchesStore();
const chapters = ref<TransformationChapterRow[]>([]);
const activeTab = ref<'chapters' | 'workflows'>('chapters');
const selectedBatchId = ref<number | null>(null);
const panelChapters = ref<TransformationChapterRow[]>([]);
const polling = ref<number | null>(null);

async function loadChapters() {
  chapters.value = await listTransformationChapters(tnId.value);
}

async function loadBatches() {
  await batchesStore.loadByTn(tnId.value);
}

function openBatchPanel(batch: Batch) {
  selectedBatchId.value = batch.id;
  panelChapters.value = chapters.value.filter((c) => c.batch_id === batch.id);
}

onMounted(async () => {
  await Promise.all([loadChapters(), loadBatches()]);
  polling.value = window.setInterval(() => {
    loadBatches();
  }, 5000);
});

onUnmounted(() => {
  if (polling.value !== null) window.clearInterval(polling.value);
});

const batches = computed<Batch[]>(() => batchesStore.byTn.get(tnId.value) ?? []);
</script>

<template>
  <div class="tn-detail">
    <header class="header">
      <h1>转换工程详情</h1>
      <p class="subtitle">TN #{{ tnId }}</p>
      <button class="btn" @click="router.back()">← 返回</button>
    </header>

    <div class="tabs">
      <button :class="{ active: activeTab === 'chapters' }" @click="activeTab = 'chapters'">
        章节一览
      </button>
      <button :class="{ active: activeTab === 'workflows' }" @click="activeTab = 'workflows'">
        工作流
      </button>
    </div>

    <table v-if="activeTab === 'chapters'" class="chapter-table">
      <thead>
        <tr>
          <th>#</th>
          <th>标题</th>
          <th>模式</th>
          <th>状态</th>
          <th>批号</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="c in chapters" :key="c.id">
          <td>{{ c.chapter_idx }}</td>
          <td>{{ c.chapter_title }}</td>
          <td>{{ c.mode }}</td>
          <td>{{ c.status }}</td>
          <td>{{ c.batch_id ?? '—' }}</td>
        </tr>
      </tbody>
    </table>

    <div v-else>
      <div v-if="batches.find((b) => b.status === 'stopped')" class="paused-banner">
        ⚠ 有工作流处于停止状态,请处理(Task 10 替换为详情页)
      </div>
      <table class="batch-table">
        <thead>
          <tr>
            <th>Label</th>
            <th>状态</th>
            <th>创建</th>
            <th>结束</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="b in batches" :key="b.id" @click="openBatchPanel(b)">
            <td>{{ b.label ?? '—' }}</td>
            <td>{{ b.status }}</td>
            <td>{{ b.created_at }}</td>
            <td>{{ b.ended_at ?? '—' }}</td>
          </tr>
        </tbody>
      </table>

      <div v-if="selectedBatchId !== null" class="side-panel">
        <div class="panel-header">
          <h3>批号 #{{ selectedBatchId }} 章节进度</h3>
          <button @click="selectedBatchId = null">关闭</button>
        </div>
        <table>
          <thead>
            <tr><th>#</th><th>标题</th><th>状态</th><th>错误</th></tr>
          </thead>
          <tbody>
            <tr v-for="c in panelChapters" :key="c.id">
              <td>{{ c.chapter_idx }}</td>
              <td>{{ c.chapter_title }}</td>
              <td>{{ c.status }}</td>
              <td>{{ c.error ?? '' }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tn-detail { padding: 16px; }
.header { display: flex; align-items: center; gap: 12px; margin-bottom: 16px; }
.header h1 { margin: 0; font-size: 20px; }
.subtitle { margin: 0; color: var(--color-text-muted, #888); }
.btn { padding: 6px 12px; }
.tabs { display: flex; gap: 8px; margin-bottom: 16px; }
.tabs button { padding: 6px 14px; }
.tabs button.active { background: var(--color-primary, #4a90e2); color: white; }
.chapter-table, .batch-table { width: 100%; border-collapse: collapse; }
.chapter-table th, .chapter-table td,
.batch-table th, .batch-table td { padding: 6px 10px; border-bottom: 1px solid var(--color-border, #eee); text-align: left; }
.paused-banner { background: var(--color-error-bg, #fee); padding: 8px 12px; margin-bottom: 12px; border-radius: 4px; }
.side-panel { position: fixed; top: 0; right: 0; width: 360px; height: 100vh; background: var(--color-bg, #fff); border-left: 1px solid var(--color-border, #eee); padding: 16px; overflow: auto; box-shadow: -2px 0 8px rgba(0,0,0,0.1); }
.panel-header { display: flex; justify-content: space-between; align-items: center; }
</style>