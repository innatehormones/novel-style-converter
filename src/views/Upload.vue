<template>
  <section class="upload">
    <header class="header">
      <Button @click="onBack">← 返回</Button>
      <h2>{{ filename || '加载中...' }}</h2>
      <Button :loading="saving" :disabled="!dirty || hasDataAsset" :title="hasDataAsset ? '原文已有关联数据资产,无法修改。请先在数据资产页删除。' : ''" @click="save">保存</Button>
      <Button
        :disabled="uploadId == null || dirty || hasDataAsset"
        :title="hasDataAsset ? '数据资产已存在,请先在数据资产页删除再清洗(清洗会改变原文字节数,但 chapters.byte_range 不会自动重算)' : ''"
        @click="openCleaning"
      >清洗</Button>
      <Button kind="primary" :disabled="uploadId == null || dirty" @click="goParse">转为数据资产</Button>
    </header>
    <div v-if="error" class="alert">{{ error }}</div>
    <div v-if="uploadId != null" class="meta-strip">
      <div class="tags">
        <Tag>实体文件</Tag>
        <Tag v-if="hasDataAsset" kind="success">已解析</Tag>
      </div>
      <span class="meta-text" :title="metaTooltip">
        {{ mbSize }} MB · {{ lineCount }} 行 · {{ charCount }} 字
      </span>
    </div>
    <div class="body">
      <textarea
        v-model="rawText"
        class="raw"
        spellcheck="false"
        :readonly="hasDataAsset"
      />
    </div>
    <CleaningDialog
      v-model:open="cleaningOpen"
      :source-text="rawText"
      @confirm="onCleaningConfirm"
    />

    <ConfirmDialog
      v-model:open="resplitConfirmOpen"
      title="重新解析"
      message="清洗会破坏现有章节范围,需要重新解析。是否继续?"
      @confirm="doOpenCleaning"
    />

    <AlertDialog
      v-model:open="alertOpen"
      :title="alertTitle"
      :message="alertMessage"
    />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import Button from '../components/ui/Button.vue';
import Tag from '../components/ui/Tag.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import AlertDialog from '../components/ui/AlertDialog.vue';
import { getUpload, getUploadText, updateUploadText, findDataAssetByUpload } from '../ipc/commands';
import CleaningDialog from '../components/CleaningDialog.vue';

const route = useRoute();
const router = useRouter();
const uploadId = ref<number | null>(null);
const filename = ref('');
const sha256 = ref('');
const byteSize = ref(0);
const rawText = ref('');
const savedText = ref('');
const saving = ref(false);
const error = ref<string | null>(null);
const cleaningOpen = ref(false);
const hasDataAsset = ref(false);
const resplitConfirmOpen = ref(false);
const alertOpen = ref(false);
const alertTitle = ref('提示');
const alertMessage = ref('');

onMounted(async () => {
  const id = Number(route.params.uploadId);
  if (!Number.isFinite(id) || id <= 0) {
    error.value = `无效的上传 ID: ${String(route.params.uploadId)}`;
    return;
  }
  uploadId.value = id;
  try {
    const [meta, text, existingDaId] = await Promise.all([
      getUpload(id),
      getUploadText(id),
      findDataAssetByUpload(id),
    ]);
    filename.value = meta?.filename ?? '';
    sha256.value = meta?.sha256 ?? '';
    byteSize.value = meta?.byte_size ?? 0;
    rawText.value = text;
    savedText.value = text;
    hasDataAsset.value = existingDaId != null;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  }
});

const dirty = computed(() => rawText.value !== savedText.value);
const lineCount = computed(() => rawText.value.split(/\r\n|\n|\r/).length);
const charCount = computed(() => [...rawText.value].length);
const mbSize = computed(() => (byteSize.value / 1024 / 1024).toFixed(2));
const metaTooltip = computed(() => (sha256.value ? `SHA256: ${sha256.value}` : ''));

function onBack() {
  void router.push('/uploads');
}

async function save() {
  if (uploadId.value == null || !dirty.value) return;
  saving.value = true;
  error.value = null;
  try {
    await updateUploadText(uploadId.value, rawText.value);
    savedText.value = rawText.value;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    saving.value = false;
  }
}

function goParse() {
  if (uploadId.value == null) return;
  void router.push({ name: 'parse-wizard', params: { uploadId: uploadId.value } });
}

async function openCleaning() {
  if (uploadId.value == null) return;
  if (rawText.value.length > 10 * 1024 * 1024) {
    alertMessage.value = '文本过大,请先手动精简';
    alertOpen.value = true;
    return;
  }
  try {
    const existing = await findDataAssetByUpload(uploadId.value);
    if (existing != null) {
      resplitConfirmOpen.value = true;
      return;
    }
  } catch (e: unknown) {
    alertMessage.value = e instanceof Error ? e.message : String(e);
    alertOpen.value = true;
    return;
  }
  cleaningOpen.value = true;
}

function doOpenCleaning() {
  cleaningOpen.value = true;
}

function onCleaningConfirm(cleanedText: string) {
  rawText.value = cleanedText;
  // savedText 不动 → dirty 变为 true;与手动编辑走同一保存路径。
}
</script>

<style scoped>
.upload {
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
}
.header h2 {
  margin: 0;
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 16px;
  font-weight: var(--font-weight-medium);
}
.meta-strip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 0;
}
.tags {
  display: flex;
  align-items: center;
  gap: 6px;
}
.meta-text {
  font-size: 12px;
  color: var(--text-secondary);
  white-space: nowrap;
  cursor: default;
}
.body {
  display: flex;
  gap: 16px;
  flex: 1;
  min-height: 0;
  margin-top: 8px;
}
.raw {
  flex: 1;
  padding: 12px;
  font-family: ui-monospace, monospace;
  font-size: 13px;
  background: var(--color-sheet);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  resize: none;
  outline: none;
  color: var(--text-primary);
}
.raw:focus {
  border-color: var(--border-strong);
}
.alert {
  margin-top: 12px;
  padding: 8px 12px;
  background: var(--color-paper-mist);
  color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin);
  font-size: 13px;
}
</style>