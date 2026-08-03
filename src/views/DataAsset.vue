<template>
  <section class="data-asset">
    <header class="header">
      <Button @click="onBack">← 返回</Button>
      <h2>{{ store.title || '加载中...' }}</h2>
      <span class="badge" :class="{ locked: !!store.lockedAt }">
        {{ store.lockedAt ? '已锁定' : '已解析' }}
      </span>
      <span v-if="store.parsedAt" class="src">{{ formatTime(store.parsedAt) }}</span>
      <Button
        class="del-btn"
        kind="danger"
        :disabled="!!store.lockedAt"
        :title="store.lockedAt ? '数据资产已锁定,无法删除' : ''"
        @click="onDelete"
      >删除资产</Button>
    </header>

    <div v-if="store.error" class="alert">{{ store.error }}</div>

    <ConfirmDialog
      v-model:open="confirmOpen"
      title="删除数据资产"
      :message="confirmMessage"
      kind="danger"
      confirm-text="删除"
      @confirm="doDelete"
    />

    <AlertDialog
      v-model:open="alertOpen"
      title="提示"
      :message="alertMessage"
    />

    <div class="panes">
      <div class="pane">
        <div class="pane-title">章节 ({{ store.chapters.length }})</div>
        <RecycleScroller
          v-if="store.chapters.length > 0"
          class="scroller"
          :items="store.chapters"
          :item-size="40"
          key-field="byte_start"
        >
          <template #default="{ item, index }">
            <div
              class="chap-row"
              :class="{ active: store.selectedIdx === index }"
              @click="store.selectChapter(index)"
            >
              <span class="idx">{{ index + 1 }}</span>
              <span class="title">{{ item.title }}</span>
              <span class="size">{{ item.word_count }} 字</span>
            </div>
          </template>
        </RecycleScroller>
        <div v-else class="empty">暂无章节</div>
      </div>
      <div class="pane">
        <div class="pane-title">原文</div>
        <pre class="content">{{ store.selectedContent }}</pre>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { RecycleScroller } from 'vue-virtual-scroller';
import Button from '../components/ui/Button.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import AlertDialog from '../components/ui/AlertDialog.vue';
import { useDataAssetStore } from '../stores/dataAsset';
import { useLibraryStore } from '../stores/library';
import { formatTime } from '../utils/format';

const route = useRoute();
const router = useRouter();
const store = useDataAssetStore();
const library = useLibraryStore();

const confirmOpen = ref(false);
const confirmMessage = computed(() => `确认删除数据资产 "${store.title}"?解析出的章节将一并删除,删除后可重新解析。`);
const alertOpen = ref(false);
const alertMessage = ref('');

onMounted(async () => {
  const raw = route.params.dataAssetId;
  const id = Number(raw);
  if (!Number.isFinite(id) || id <= 0) {
    store.error = `无效的 data_asset ID: ${String(raw)}`;
    return;
  }
  await store.load(id);
  store.selectFirstIfNone();
});

async function onDelete() {
  if (store.lockedAt) return;
  confirmOpen.value = true;
}

async function doDelete() {
  const id = store.dataAssetId;
  if (id == null) return;
  try {
    await library.removeDataAsset(id);
    void router.push('/data-assets');
  } catch (e: unknown) {
    alertMessage.value = e instanceof Error ? e.message : String(e);
    alertOpen.value = true;
  }
}

function onBack() {
  void router.push('/data-assets');
}
</script>

<style scoped>
.data-asset {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--border-color);
  min-width: 0;
}
.header h2 {
  margin: 0;
  flex: 1;
  font-size: 16px;
  font-weight: var(--font-weight-medium);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.badge {
  padding: 2px 8px;
  background: var(--bg-hover);
  border-radius: 4px;
  font-size: 12px;
  color: var(--text-secondary);
}
.badge.locked {
  background: var(--color-paper-mist);
  color: var(--color-cinnabar-deep);
}
.src {
  font-size: 12px;
  color: var(--text-secondary);
}
.del-btn {
  margin-left: auto;
}
.alert {
  margin-top: 12px;
  padding: 8px 12px;
  background: var(--bg-hover);
  color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin);
  font-size: 13px;
}
.panes {
  display: flex;
  gap: 16px;
  flex: 1;
  min-height: 0;
  margin-top: 12px;
}
.pane:first-child {
  flex: 0 0 280px;
}
.pane {
  flex: 1;
  width: 50%;
  display: flex;
  flex-direction: column;
  background: var(--color-sheet);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  overflow: hidden;
}
.pane-title {
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-color);
  font-size: 13px;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.scroller {
  flex: 1;
  overflow-y: auto;
}
.chap-row {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 40px;
  padding: 0 12px;
  border-bottom: 1px solid var(--border-color);
  cursor: pointer;
}
.chap-row:hover { background: var(--bg-hover); }
.chap-row.active {
  background: var(--color-paper-mist);
}
.idx {
  color: var(--text-secondary);
  font-size: 12px;
  min-width: 32px;
  flex-shrink: 0;
}
.title {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.size {
  color: var(--text-secondary);
  font-size: 12px;
  flex-shrink: 0;
}
.content {
  flex: 1;
  margin: 0;
  padding: 12px;
  font-family: ui-monospace, monospace;
  font-size: 13px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
  overflow-y: auto;
  color: var(--text-primary);
}
.empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-secondary);
  font-size: 13px;
}
</style>