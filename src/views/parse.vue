<template>
  <section class="chapters">
    <PageHeader title="章节解析" subtitle="调整章节 marker,提交为数据资产">
      <template #actions>
        <Button @click="onReset" :disabled="!store.dirty">重置 marker</Button>
        <Button
          v-if="store.committed && !store.dirty"
          title="丢弃已保存的章节,重新走 splitter"
          @click="onResplit"
        >重新切分</Button>
        <Button kind="primary" :loading="committing" :disabled="committing || store.workingChapters.length === 0" @click="onCommit">保存为数据资产</Button>
      </template>
    </PageHeader>

    <div v-if="store.error" class="alert">{{ store.error }}</div>
    <div v-else-if="store.committed && !store.dirty" class="info">
      当前是已保存的章节。如需重新解析,请在 DataAsset 页先删除已有数据资产。
    </div>

    <div class="panes">
      <div class="pane">
        <div class="pane-title">章节列表({{ store.workingChapters.length }})</div>
        <DynamicScroller
          class="scroller"
          :items="store.workingChapters"
          :min-item-size="48"
          key-field="byte_start"
        >
          <template #default="{ item, active }">
            <DynamicScrollerItem
              :item="item"
              :active="active"
              :size-dependencies="[item?.title ?? '']"
            >
              <div v-if="segIdx(item) >= 0" class="seg-row" @click="onChapterClick(item.byte_start)">
                <span class="seg-idx">{{ segIdx(item) + 1 }}</span>
                <input
                  class="seg-title"
                  :value="displayTitle(item.title)"
                  :title="displayTitle(item.title)"
                  @click.stop
                  @input="onTitleEdit(segIdx(item), ($event.target as HTMLInputElement).value)"
                />
                <span class="seg-size" :title="`${item.word_count} 字`">{{ formatWordCount(item.word_count) }}</span>
                <Button
                  kind="danger"
                  size="small"
                  :disabled="segIdx(item) <= 0"
                  title="并入上一章"
                  @click.stop="onMergeClick(segIdx(item))"
                >
                  并入上一章
                </Button>
              </div>
            </DynamicScrollerItem>
          </template>
        </DynamicScroller>
      </div>

      <div class="pane">
        <div class="pane-title">原文</div>
        <div class="search-toolbar">
          <input
            class="search-input"
            placeholder="全文搜索"
            :value="store.searchQuery"
            @input="onSearchInput(($event.target as HTMLInputElement).value)"
          />
          <span class="search-counter">{{ counterText }}</span>
          <Button size="small" :disabled="hitCount === 0" @click="onPrevHit">‹</Button>
          <Button size="small" :disabled="hitCount === 0" @click="onNextHit">›</Button>
        </div>
        <RecycleScroller
          ref="textScrollerRef"
          class="scroller"
          :items="lines"
          :item-size="24"
          key-field="byte_start"
        >
          <template #default="{ item, index }">
            <div
              class="line-row"
              :class="{
                marked: markerSet.has(item.byte_start),
                hit: hitLineIndicesSet.has(index),
                'active-hit': index === currentHitLineIndex,
              }"
            >
              <MarkerButton
                :title="markerSet.has(item.byte_start) ? '取消标记' : '在此拆分'"
                @mark="onMarkLine(item.byte_start)"
              />
              <span class="line-no">{{ index + 1 }}</span>
              <span class="line-text" :title="item.text">{{ item.text }}</span>
            </div>
          </template>
        </RecycleScroller>
      </div>
    </div>

    <Dialog v-model:open="mergeDialogOpen" title="确认合并">
      <p v-if="pendingMerge !== null">
        将『{{ displayTitle(store.workingChapters[pendingMerge]?.title) }}』并入上一章
        『{{ displayTitle(store.workingChapters[pendingMerge! - 1]?.title) }}』?
      </p>
      <p class="hint">提交章节前可点『重置 marker』撤销。</p>
      <template #footer>
        <Button @click="cancelMerge">取消</Button>
        <Button kind="danger" @click="confirmMerge">确认合并</Button>
      </template>
    </Dialog>

    <Dialog v-model:open="commitDialogOpen" title="保存为数据资产">
      <p>请输入数据资产标题:</p>
      <input
        class="title-input"
        :value="pendingTitle"
        placeholder="例如:第 1 卷"
        @input="pendingTitle = ($event.target as HTMLInputElement).value"
        @keyup.enter="confirmCommit"
      />
      <p class="hint">提交后 data_asset 即被锁定,需要重新解析会创建新 data_asset。</p>
      <template #footer>
        <Button @click="cancelCommit">取消</Button>
        <Button kind="primary" :disabled="!pendingTitle.trim()" @click="confirmCommit">确认</Button>
      </template>
    </Dialog>

    <ConfirmDialog
      v-model:open="resplitConfirmOpen"
      title="重新解析"
      message="丢弃已保存的章节,重新走 splitter?"
      kind="danger"
      confirm-text="重新切分"
      @confirm="doResplit"
    />

    <AlertDialog
      v-model:open="alertOpen"
      title="提示"
      :message="alertMessage"
    />
  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { DynamicScroller, DynamicScrollerItem, RecycleScroller } from 'vue-virtual-scroller';
import 'vue-virtual-scroller/dist/vue-virtual-scroller.css';
import Button from '../components/ui/Button.vue';
import Dialog from '../components/ui/Dialog.vue';
import PageHeader from '../components/ui/PageHeader.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import AlertDialog from '../components/ui/AlertDialog.vue';
import MarkerButton from '../components/MarkerButton.vue';
import { useChaptersStore } from '../stores/chapters';
import { useChapterSearch } from '../composables/useChapterSearch';
import { formatWordCount } from '../utils/format';
import type { ChapterSegment } from '../ipc/types';

const route = useRoute();
const router = useRouter();
const store = useChaptersStore();

const committing = ref(false);
const textScrollerRef = ref<InstanceType<typeof RecycleScroller> | null>(null);
const pendingMerge = ref<number | null>(null);
const mergeDialogOpen = computed({
  get: () => pendingMerge.value !== null,
  set: (v: boolean) => {
    if (!v) pendingMerge.value = null;
  },
});

const commitDialogOpen = ref(false);
const pendingTitle = ref('');

const resplitConfirmOpen = ref(false);
const alertOpen = ref(false);
const alertMessage = ref('');

const lines = computed(() => store.rawLines);
const markerSet = computed(() => new Set(store.markers));

const searchQueryRef = computed({
  get: () => store.searchQuery,
  set: (v: string) => store.setSearchQuery(v),
});
const search = useChapterSearch(searchQueryRef as unknown as import('vue').Ref<string>, lines);
const { hitLineIndices, hitCount, currentHitLineIndex, next, prev } = search;
const hitLineIndicesSet = computed(() => new Set(hitLineIndices.value));

const counterText = computed(() => {
  if (hitCount.value === 0) return '0 / 0';
  const cursor = hitLineIndices.value.indexOf(currentHitLineIndex.value);
  return `${cursor + 1} / ${hitCount.value}`;
});

function scrollToActiveHit() {
  void nextTick(() => {
    if (currentHitLineIndex.value >= 0) {
      textScrollerRef.value?.scrollToItem(currentHitLineIndex.value);
    }
  });
}

onMounted(() => {
  void store.load(Number(route.params.uploadId));
});

function onMarkLine(byteStart: number) {
  if (markerSet.value.has(byteStart)) {
    store.removeMarker(byteStart);
  } else {
    store.addMarker(byteStart);
  }
}

function onChapterClick(byteStart: number) {
  const idx = lines.value.findIndex((l) => l.byte_start >= byteStart);
  if (idx >= 0) textScrollerRef.value?.scrollToItem(idx);
}

function onTitleEdit(idx: number, value: string) {
  store.updateTitle(idx, value);
}

function onMergeClick(idx: number) {
  if (idx === 0) return;
  pendingMerge.value = idx;
}

function cancelMerge() {
  pendingMerge.value = null;
}

function confirmMerge() {
  if (pendingMerge.value !== null) {
    store.removeChapter(pendingMerge.value);
  }
  pendingMerge.value = null;
}

function displayTitle(t: unknown): string {
  if (typeof t !== 'string' || t.length === 0) return '(无标题)';
  return t;
}

function segIdx(item: ChapterSegment | null | undefined): number {
  if (!item) return -1;
  return store.workingChapters.findIndex((s) => s.byte_start === item.byte_start);
}

function onSearchInput(value: string) {
  store.setSearchQuery(value);
  scrollToActiveHit();
}

function onNextHit() {
  next();
  scrollToActiveHit();
}

function onPrevHit() {
  prev();
  scrollToActiveHit();
}

function onReset() {
  store.reset();
}

async function onResplit() {
  resplitConfirmOpen.value = true;
}

async function doResplit() {
  await store.reSplit();
}

function onCommit() {
  pendingTitle.value = '';
  commitDialogOpen.value = true;
}

function cancelCommit() {
  commitDialogOpen.value = false;
}

async function confirmCommit() {
  const title = pendingTitle.value.trim();
  if (!title) return;
  committing.value = true;
  commitDialogOpen.value = false;
  try {
    const newDataAssetId = await store.commit(title);
    void router.push(`/library/data/${newDataAssetId}`);
  } catch (e: unknown) {
    alertMessage.value = e instanceof Error ? e.message : String(e);
    alertOpen.value = true;
  } finally {
    committing.value = false;
  }
}
</script>

<style scoped>
.chapters {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.alert {
  padding: 12px 16px;
  background: var(--bg-hover);
  color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin);
  margin-bottom: 12px;
}
.info {
  padding: 8px 16px;
  background: var(--bg-hover);
  color: var(--text-secondary);
  border-radius: var(--radius-pin);
  margin-bottom: 12px;
  font-size: 13px;
}
.panes {
  display: flex;
  gap: 16px;
  flex: 1;
  min-height: 0;
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
.seg-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  border-bottom: 1px solid var(--border-color);
  cursor: pointer;
}
.seg-row:hover { background: var(--bg-hover); }
.seg-idx {
  color: var(--text-secondary);
  font-size: 12px;
  min-width: 32px;
  flex-shrink: 0;
}
.seg-title {
  flex: 1;
  min-width: 0;
  padding: 0 8px;
  height: 28px;
  border: 1px solid transparent;
  border-radius: var(--radius-pin);
  font-size: 13px;
  font-family: inherit;
  background: transparent;
  outline: none;
  color: var(--text-primary);
}
.seg-title:hover { border-color: var(--border-color); }
.seg-title:focus { border-color: var(--border-strong); }
.seg-size {
  color: var(--text-secondary);
  font-size: 12px;
  flex-shrink: 0;
}
.search-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}
.search-input {
  flex: 1;
  min-width: 0;
  height: 28px;
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: var(--radius-pin);
  font-size: 13px;
  font-family: inherit;
  outline: none;
  background: transparent;
  color: var(--text-primary);
}
.search-input:hover { border-color: var(--border-color); }
.search-input:focus { border-color: var(--border-strong); }
.search-counter {
  font-size: 12px;
  color: var(--text-secondary);
  min-width: 64px;
  text-align: center;
  flex-shrink: 0;
}
.line-row {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 24px;
  padding: 0 12px;
  font-size: 13px;
}
.line-row.marked { background: var(--bg-hover); }
.line-row.hit { background: var(--color-paper-mist); }
.line-row.active-hit { background: var(--color-cinnabar); color: #faf6ee; }
.line-no {
  color: var(--text-secondary);
  font-size: 11px;
  min-width: 40px;
  text-align: right;
  flex-shrink: 0;
}
.line-text {
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.hint {
  margin: 12px 0 0;
  font-size: 12px;
  color: var(--text-secondary);
}
.title-input {
  width: 100%;
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  font-size: 14px;
  font-family: inherit;
  background: var(--color-sheet);
  color: var(--text-primary);
  outline: none;
  box-sizing: border-box;
}
.title-input:focus { border-color: var(--border-strong); }
</style>