<template>
  <section>
    <PageHeader :title="pageTitle" subtitle="在这里管理上传文件、解析后的数据资产与转换小说">
      <template #actions>
        <Button v-if="page === 'uploads'" kind="primary" :loading="store.uploading" @click="uploadDialogOpen = true">上传 .txt</Button>
      </template>
    </PageHeader>

    <div v-if="store.error" class="alert">{{ store.error }}</div>

    <template v-if="page === 'uploads'">
      <div v-if="!store.loading && store.uploads.length === 0" class="empty">
        还没有文件,点击右上"上传 .txt"添加一个。
      </div>
      <Table
        v-else
        :columns="uploadColumns"
        :data="store.uploads"
        :row-key="(row) => row.id"
      >
        <template #cell-filename="{ row }">{{ row.filename }}</template>
        <template #cell-size="{ row }">{{ formatSize(row.byte_size) }}</template>
        <template #cell-words="{ row }">{{ formatWordCount(row.word_count) }}</template>
        <template #cell-uploaded="{ row }">{{ formatTime(row.uploaded_at) }}</template>
        <template #cell-actions="{ row }">
          <Button size="small" @click="goUpload(row.id)">查看</Button>
          <Button v-if="!hasDataAsset(row.id)" size="small" @click="goParse(row.id)">解析章节</Button>
          <Button size="small" kind="danger" @click="onDeleteUpload(row.id, row.filename)">删除</Button>
        </template>
      </Table>
    </template>

    <template v-else-if="page === 'data-assets'">
      <div v-if="!store.loading && store.dataAssets.length === 0" class="empty">
        还没有数据资产。请到“上传”页面选择文件并解析章节。
      </div>
      <Table
        v-else
        :columns="daColumns"
        :data="store.dataAssets"
        :row-key="(row) => row.id"
      >
        <template #cell-title="{ row }">{{ row.title }}</template>
        <template #cell-source="{ row }">
          <span class="muted">{{ row.filename }}</span>
        </template>
        <template #cell-words="{ row }">{{ formatWordCount(row.word_count) }}</template>
        <template #cell-status="{ row }">
          <Tag v-if="row.locked_at" kind="warn">已锁定</Tag>
          <Tag v-else kind="success">可重解析</Tag>
        </template>
        <template #cell-parsed="{ row }">{{ formatTime(row.parsed_at) }}</template>
        <template #cell-actions="{ row }">
          <Button size="small" @click="goDataAsset(row.id)">打开</Button>
          <Button size="small" @click="openCreateTn(row.id)">转换</Button>
          <Button size="small" kind="danger" :disabled="!!row.locked_at" :title="row.locked_at ? 'data_asset 已锁定,无法删除' : ''" @click="onDeleteDa(row.id, row.title)">删除</Button>
        </template>
      </Table>
    </template>

    <template v-else>
      <div v-if="!store.loading && store.transformationNovels.length === 0" class="empty">
        还没有转换小说。请到“数据资产”页面选择资产并新建转换。
      </div>
      <Table
        v-else
        :columns="tnColumns"
        :data="store.transformationNovels"
        :row-key="(row) => row.id"
      >
        <template #cell-id="{ row }">{{ row.id }}</template>
        <template #cell-title="{ row }">
          <Input v-if="renamingId === row.id" v-model="renameDraft" />
          <template v-else>{{ row.title }}</template>
        </template>
        <template #cell-source="{ row }">
          <span class="muted">data_asset #{{ row.data_asset_id }} · {{ row.chapters_count }} 章</span>
        </template>
        <template #cell-created="{ row }">{{ formatTime(row.created_at) }}</template>
        <template #cell-actions="{ row }">
          <template v-if="renamingId === row.id">
            <Button size="small" kind="primary" @click="onSaveRename(row.id)">保存</Button>
            <Button size="small" @click="cancelRename">取消</Button>
          </template>
          <template v-else>
            <Button size="small" kind="primary" @click="openCreateBatch(row)">▶ 新建工作流</Button>
            <Button size="small" @click="goDetail(row.id)">详情</Button>
            <Button size="small" @click="startRename(row.id, row.title)">重命名</Button>
            <Button size="small" kind="danger" @click="onDeleteTn(row.id, row.title)">删除</Button>
          </template>
        </template>
      </Table>
    </template>

    <UploadDialog v-model:open="uploadDialogOpen" @submit="onUpload" />

    <TransformationNovelDialog
      v-model:open="tnDialogOpen"
      :data-asset-id="tnDialogDataAssetId"
      @submit="onCreateTn"
    />

    <CreateBatchDialog
      v-model:open="createBatchOpen"
      :tn-id="createBatchTnId"
      :default-prompt-id="createBatchDefaults.default_prompt_id"
      :default-model-config-id="createBatchDefaults.default_model_config_id"
      :default-mode="createBatchDefaults.default_mode"
      @submit="onCreateBatch"
    />

    <ConfirmDialog
      v-model:open="deleteUploadConfirmOpen"
      title="删除上传"
      :message="deleteUploadMessage"
      kind="danger"
      confirm-text="删除"
      @confirm="doDeleteUpload"
    />

    <ConfirmDialog
      v-model:open="deleteTnConfirmOpen"
      title="删除转换小说"
      :message="deleteTnMessage"
      kind="danger"
      confirm-text="删除"
      @confirm="doDeleteTn"
    />

    <ConfirmDialog
      v-model:open="deleteDaConfirmOpen"
      title="删除数据资产"
      :message="deleteDaMessage"
      kind="danger"
      confirm-text="删除"
      @confirm="doDeleteDa"
    />

    <AlertDialog
      v-model:open="alertOpen"
      :title="alertTitle"
      :message="alertMessage"
    />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import Button from '../components/ui/Button.vue';
import Input from '../components/ui/Input.vue';
import PageHeader from '../components/ui/PageHeader.vue';
import Table from '../components/ui/Table.vue';
import Tag from '../components/ui/Tag.vue';
import UploadDialog from '../components/UploadDialog.vue';
import TransformationNovelDialog from '../components/TransformationNovelDialog.vue';
import CreateBatchDialog from '../components/CreateBatchDialog.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import AlertDialog from '../components/ui/AlertDialog.vue';
import { useLibraryStore } from '../stores/library';
import { useBatchesStore } from '../stores/batches';
import { createWorkflow, listTransformationSourceChapters } from '../ipc/commands';
import { formatSize, formatTime, formatWordCount } from '../utils/format';

const route = useRoute();
const router = useRouter();
const store = useLibraryStore();
const batchesStore = useBatchesStore();
type Page = 'uploads' | 'data-assets' | 'transformations';
const page = computed<Page>(() => (route.meta.libraryPage as Page | undefined) ?? 'uploads');
const pageTitle = computed(() => ({
  uploads: '上传',
  'data-assets': '数据资产',
  transformations: '转换',
})[page.value]);

const uploadDialogOpen = ref(false);
const tnDialogOpen = ref(false);
const tnDialogDataAssetId = ref(0);
const renamingId = ref<number | null>(null);
const renameDraft = ref('');

const createBatchOpen = ref(false);
const createBatchTnId = ref(0);
const createBatchDefaults = ref<{
  default_prompt_id: number | null;
  default_model_config_id: number | null;
  default_mode: 'compress' | 'style' | null;
}>({ default_prompt_id: null, default_model_config_id: null, default_mode: null });

const deleteUploadConfirmOpen = ref(false);
const deleteUploadMessage = ref('');
const deleteUploadId = ref<number | null>(null);

const deleteTnConfirmOpen = ref(false);
const deleteTnMessage = ref('');
const deleteTnId = ref<number | null>(null);

const deleteDaConfirmOpen = ref(false);
const deleteDaMessage = ref('');
const deleteDaId = ref<number | null>(null);

const alertOpen = ref(false);
const alertTitle = ref('提示');
const alertMessage = ref('');

function showAlert(title: string, message: string) {
  alertTitle.value = title;
  alertMessage.value = message;
  alertOpen.value = true;
}

const uploadColumns = [
  { key: 'filename', title: '文件名', width: '260px' },
  { key: 'size', title: '大小', width: '100px' },
  { key: 'words', title: '字数', width: '100px' },
  { key: 'uploaded', title: '上传时间', width: '180px' },
  { key: 'actions', title: '操作', width: '260px', type: 'actions' as const },
];

const daColumns = [
  { key: 'title', title: '标题', width: '220px' },
  { key: 'source', title: '来源', width: '260px' },
  { key: 'words', title: '字数', width: '100px' },
  { key: 'status', title: '状态', width: '120px' },
  { key: 'parsed', title: '解析时间', width: '180px' },
  { key: 'actions', title: '操作', width: '200px', type: 'actions' as const },
];

const tnColumns = [
  { key: 'id', title: 'id', width: '60px' },
  { key: 'title', title: '标题', width: '220px' },
  { key: 'source', title: '源', width: '240px' },
  { key: 'created', title: '创建时间', width: '180px' },
  { key: 'actions', title: '操作', width: '280px', type: 'actions' as const },
];

onMounted(() => store.load());
/// Library 同一组件实例服务 3 个 tab(uploads / data-assets / transformations),
/// vue-router 切换 path 不重新 mount,onMounted 只跑一次。
/// 用户在 DataAsset.vue 删除后再切到 /data-assets,得 reload 才能拿到新数据。
watch(() => route.path, () => store.load());

/// 上传列表每行都要查"是否已有 data_asset",data_assets 数量是 O(N),
/// 若每次都 .some() 则渲染总复杂度 O(M*N)。用 Set 一次性算好,O(1) 查。
const uploadIdsWithDataAsset = computed(
  () => new Set(store.dataAssets.map((d) => d.upload_id)),
);
function hasDataAsset(uploadId: number): boolean {
  return uploadIdsWithDataAsset.value.has(uploadId);
}

function openCreateTn(dataAssetId: number) {
  tnDialogDataAssetId.value = dataAssetId;
  tnDialogOpen.value = true;
}

// onUpload is the translation layer between Vue-camelCase dialog emit
// and snake-case IPC DTO. Dialog emits { filePath, filename }; we re-pack
// to { file_path, filename } before handing to the store / IPC.
async function onUpload(input: { filePath: string; filename: string }) {
  try {
    await store.upload({ file_path: input.filePath, filename: input.filename });
  } catch (e: unknown) {
    showAlert('提示', e instanceof Error ? e.message : String(e));
  }
}

async function onDeleteUpload(id: number, filename: string) {
  deleteUploadId.value = id;
  deleteUploadMessage.value = `确认删除文件 "${filename}"?`;
  deleteUploadConfirmOpen.value = true;
}

async function doDeleteUpload() {
  const id = deleteUploadId.value;
  if (id == null) return;
  try {
    await store.removeUpload(id);
  } catch (e: unknown) {
    showAlert('提示', e instanceof Error ? e.message : String(e));
  }
}

async function onCreateTn(input: { data_asset_id: number; title: string }) {
  try {
    await store.createTransformationNovel(input);
  } catch (e: unknown) {
    showAlert('提示', e instanceof Error ? e.message : String(e));
  }
}

function startRename(id: number, title: string) {
  renamingId.value = id;
  renameDraft.value = title;
}

function cancelRename() {
  renamingId.value = null;
}

async function onSaveRename(id: number) {
  const t = renameDraft.value.trim();
  if (t === '') {
    showAlert('标题不能为空', '请输入转换小说标题。');
    return;
  }
  try {
    await store.renameTransformationNovel({ id, title: t });
    renamingId.value = null;
  } catch (e: unknown) {
    showAlert('提示', e instanceof Error ? e.message : String(e));
  }
}

async function onDeleteTn(id: number, title: string) {
  deleteTnId.value = id;
  deleteTnMessage.value = `确认删除转换小说 "${title}"?历史转换结果一并删除。`;
  deleteTnConfirmOpen.value = true;
}

async function doDeleteTn() {
  const id = deleteTnId.value;
  if (id == null) return;
  try {
    await store.removeTransformationNovel(id);
  } catch (e: unknown) {
    showAlert('提示', e instanceof Error ? e.message : String(e));
  }
}

async function onDeleteDa(id: number, title: string) {
  deleteDaId.value = id;
  deleteDaMessage.value = `确认删除数据资产 "${title}"?解析出的章节将一并删除。`;
  deleteDaConfirmOpen.value = true;
}

async function doDeleteDa() {
  const id = deleteDaId.value;
  if (id == null) return;
  try {
    await store.removeDataAsset(id);
  } catch (e: unknown) {
    showAlert('提示', e instanceof Error ? e.message : String(e));
  }
}

function openCreateBatch(row: {
  id: number;
  default_prompt_id: number | null;
  default_model_config_id: number | null;
  default_mode: 'compress' | 'style' | null;
}) {
  createBatchTnId.value = row.id;
  createBatchDefaults.value = {
    default_prompt_id: row.default_prompt_id,
    default_model_config_id: row.default_model_config_id,
    default_mode: row.default_mode,
  };
  createBatchOpen.value = true;
}

async function onCreateBatch(input: {
  label: string | null;
  on_failure_policy: 'pause_and_review' | 'terminate' | 'skip_failed';
  overrides: {
    prompt_id: number;
    model_config_id: number;
    mode: 'compress' | 'style';
    ctx_prev_original: number;
    ctx_prev_transformed: number;
    ctx_next_original: number;
  };
}) {
  try {
    // 旧 create_batch+dispatch_batch 隐含"处理 tn 全部章节"语义;这里
    // 把 source_chapters 拉全 → create_workflow,让 Task 9 之前不破流程。
    // Task 10 的 CreateBatchDialog 改造后会改成多选 chapter_ids。
    const sources = await listTransformationSourceChapters(createBatchTnId.value);
    const chapterIds = sources.map((s) => s.chapter_id);
    if (chapterIds.length === 0) {
      showAlert('创建工作流失败', '该转换小说没有可处理的章节。');
      return;
    }
    const workflow = await createWorkflow({
      tn_id: createBatchTnId.value,
      label: input.label,
      chapter_ids: chapterIds,
      prompt_id: input.overrides.prompt_id,
      model_config_id: input.overrides.model_config_id,
      mode: input.overrides.mode,
      ctx_prev_original: input.overrides.ctx_prev_original,
      ctx_prev_transformed: input.overrides.ctx_prev_transformed,
      ctx_next_original: input.overrides.ctx_next_original,
    });
    void batchesStore.refresh(workflow.id);
    void router.push({ name: 'transformation-detail', params: { tnId: String(workflow.tn_id) } });
  } catch (e: unknown) {
    showAlert('创建工作流失败', e instanceof Error ? e.message : String(e));
  }
}

function goUpload(id: number) {
  void router.push({ name: 'upload', params: { uploadId: id } });
}

function goDataAsset(id: number) {
  void router.push({ name: 'data-asset', params: { dataAssetId: id } });
}

function goParse(uploadId: number) {
  void router.push({ name: 'parse-wizard', params: { uploadId } });
}

function goDetail(tnId: number) {
  void router.push({ name: 'transformation-detail', params: { tnId: String(tnId) } });
}
</script>

<style scoped>
.alert {
  padding: 12px 16px;
  background: var(--bg-hover);
  color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin);
  margin-bottom: 16px;
}
.empty {
  text-align: center;
  padding: 56px 0;
  color: var(--text-secondary);
  border: 1px dashed var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
}
.muted {
  color: var(--text-secondary);
  font-size: 12px;
}
</style>