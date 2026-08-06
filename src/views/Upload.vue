<template>
  <section class="upload">
    <PageHeader :title="filename || '加载中...'" size="small">
      <template #back>
        <Button aria-label="返回" @click="onBack">
          <IconArrowLeft :size="16" :stroke-width="1.5" />
        </Button>
      </template>
      <template #actions>
        <Button :loading="saving" :disabled="!dirty" @click="save">保存</Button>
        <Button
          :disabled="uploadId == null || dirty"
          @click="openCleaning"
        >清洗</Button>
        <Button kind="primary" :disabled="uploadId == null || dirty" @click="goParse">转为数据资产</Button>
      </template>
    </PageHeader>
    <div v-if="error" class="alert">{{ error }}</div>
    <div v-if="uploadId != null" class="meta-strip">
      <div class="tags">
        <Tag>实体文件</Tag>
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
      />
    </div>
    <CleaningDialog
      v-model:open="cleaningOpen"
      :source-text="rawText"
      @confirm="onCleaningConfirm"
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
import PageHeader from '../components/ui/PageHeader.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import AlertDialog from '../components/ui/AlertDialog.vue';
import IconArrowLeft from '~icons/lucide/arrow-left';
import { getUpload, getUploadText, updateUploadText } from '../ipc/commands';
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
    const [meta, text] = await Promise.all([
      getUpload(id),
      getUploadText(id),
    ]);
    filename.value = meta?.filename ?? '';
    sha256.value = meta?.sha256 ?? '';
    byteSize.value = meta?.byte_size ?? 0;
    rawText.value = text;
    savedText.value = text;
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
  // 新设计:清洗只改 uploads.original_text,不影响已有 chapters.body,无需二次确认。
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