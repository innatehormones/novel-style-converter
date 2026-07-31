import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import type {
  ChapterSegment,
  ChapterInput,
} from '../ipc/types';
import {
  commitDataAsset as ipcCommitDataAsset,
  getUploadText as ipcGetUploadText,
  getUpload as ipcGetUpload,
  listChapterSegments as ipcListChapterSegments,
  listCommittedSegments as ipcListCommittedSegments,
} from '../ipc/commands';

/// source: 进入页面时锁定的初始章节列表。
///   - committed: 从 chapters 表读,代表用户上次 commit 的成果。
///   - fresh: splitter 首次跑,代表用户还未 commit。
type SourceKind = 'committed' | 'fresh';

/// 把字符串切成 {byte_start, text} 行。byte_start 用 UTF-8 字节偏移,
/// 后续 marker 用的也是字节偏移,确保 byte_start 对齐 chapter.byte_start。
function computeLines(text: string): { byte_start: number; text: string }[] {
  const textLines = text.split('\n');
  const bytes = new TextEncoder().encode(text);
  const nlPos: number[] = [];
  let i = bytes.indexOf(0x0a);
  while (i >= 0) {
    nlPos.push(i);
    i = bytes.indexOf(0x0a, i + 1);
  }
  const out: { byte_start: number; text: string }[] = [];
  let byteStart = 0;
  for (let li = 0; li < textLines.length; li++) {
    out.push({ byte_start: byteStart, text: textLines[li] });
    if (li < nlPos.length) byteStart = nlPos[li] + 1;
  }
  return out;
}

export const useChaptersStore = defineStore('chapters', () => {
  const uploadId = ref<number | null>(null);
  const rawText = ref<string>('');
  const filename = ref<string>('');

  /// rawLines: 用于 parse.vue 右栏行号视图。byte_start 对齐 chapter.byte_start。
  const rawLines = computed(() => computeLines(rawText.value));

  const source = ref<ChapterSegment[]>([]);
  const sourceKind = ref<SourceKind | null>(null);
  const markers = ref<number[]>([]);
  const suppressed = ref<number[]>([]);
  const titleOverrides = ref<Record<number, string>>({});

  /// workingChapters: 当前 UI 实际渲染的章节列表。
  ///   - 没有 edits 时 = source(可能已含 titleOverrides 叠加)
  ///   - 有 markers/suppressed 时走 splitter(markers, suppressed)
  ///   - titleOverrides 永远最后应用(本地立即生效,不需 splitter)
  const workingChapters = ref<ChapterSegment[]>([]);

  const loading = ref(false);
  const error = ref<string | null>(null);
  const searchQuery = ref<string>('');
  const currentHitIndex = ref<number>(0);

  let debounceHandle: number | null = null;
  let requestToken = 0;

  const committed = computed(() => sourceKind.value === 'committed');

  /// edits(markers/suppressed/titleOverrides)与 source 是否存在差异。
  const dirty = computed(() => {
    if (markers.value.length > 0 || suppressed.value.length > 0) return true;
    for (const byteStartStr in titleOverrides.value) {
      const byteStart = Number(byteStartStr);
      const seg = source.value.find((s) => s.byte_start === byteStart);
      if (!seg || titleOverrides.value[byteStart] !== seg.title) return true;
    }
    return false;
  });

  /// 拿当前 edits(markers/suppressed)走 splitter,或退回 source。
  async function applyWorking(token: number) {
    if (uploadId.value === null) return;
    const id = uploadId.value;
    let segs: ChapterSegment[];
    if (markers.value.length > 0 || suppressed.value.length > 0) {
      segs = await ipcListChapterSegments(id, markers.value, suppressed.value);
    } else {
      segs = source.value;
    }
    if (token !== requestToken) return;
    workingChapters.value = applyTitleOverrides(segs);
  }

  function applyTitleOverrides(segs: ChapterSegment[]): ChapterSegment[] {
    if (!segs) return [];
    const overrides = titleOverrides.value;
    return segs.map((s) => {
      const t = overrides[s.byte_start];
      const title = t !== undefined && t !== s.title ? t : s.title;
      return { ...s, title };
    });
  }

  function scheduleRecompute() {
    if (debounceHandle !== null) clearTimeout(debounceHandle);
    debounceHandle = window.setTimeout(() => {
      void recompute();
    }, 200);
  }

  async function recompute() {
    const token = ++requestToken;
    await applyWorking(token);
  }

  async function load(id: number) {
    uploadId.value = id;
    /// 用户可见数据同步清空,避免切小说时仍渲染上一个小说的内容。
    workingChapters.value = [];
    source.value = [];
    sourceKind.value = null;
    rawText.value = '';
    filename.value = '';
    markers.value = [];
    suppressed.value = [];
    titleOverrides.value = {};
    searchQuery.value = '';
    currentHitIndex.value = 0;
    if (debounceHandle !== null) {
      clearTimeout(debounceHandle);
      debounceHandle = null;
    }
    ++requestToken;
    const token = requestToken;
    loading.value = true;
    error.value = null;
    try {
      const [text, committedSegs, meta] = await Promise.all([
        ipcGetUploadText(id),
        ipcListCommittedSegments(id).catch(() => []),
        ipcGetUpload(id).catch(() => null),
      ]);
      if (token !== requestToken) return;
      rawText.value = text;
      filename.value = meta?.filename ?? '';
      let segs: ChapterSegment[];
      let kind: SourceKind;
      if (committedSegs.length > 0) {
        segs = committedSegs;
        kind = 'committed';
      } else {
        segs = await ipcListChapterSegments(id, [], []);
        kind = 'fresh';
      }
      if (token !== requestToken) return;
      source.value = segs;
      sourceKind.value = kind;
      await applyWorking(token);
    } catch (e: unknown) {
      if (token === requestToken) error.value = e instanceof Error ? e.message : String(e);
    } finally {
      if (token === requestToken) loading.value = false;
    }
  }

  function addMarker(pos: number) {
    if (markers.value.includes(pos)) return;
    if (suppressed.value.includes(pos)) {
      suppressed.value = suppressed.value.filter((p) => p !== pos);
    }
    markers.value = [...markers.value, pos].sort((a, b) => a - b);
    scheduleRecompute();
  }

  function removeMarker(pos: number) {
    if (!markers.value.includes(pos)) return;
    markers.value = markers.value.filter((m) => m !== pos);
    scheduleRecompute();
  }

  /// 并入上一章:在 suppressed 加 workingChapters[idx].byte_start。
  function removeChapter(idx: number) {
    const seg = workingChapters.value[idx];
    if (!seg || idx === 0) return;
    if (!suppressed.value.includes(seg.byte_start)) {
      suppressed.value = [...suppressed.value, seg.byte_start].sort((a, b) => a - b);
      scheduleRecompute();
    }
  }

  /// 标题编辑:本地立即更新 workingChapters;同时记录 override。
  function updateTitle(idx: number, title: string) {
    const seg = workingChapters.value[idx];
    if (!seg) return;
    seg.title = title;
    const original = source.value.find((s) => s.byte_start === seg.byte_start)?.title;
    const next = { ...titleOverrides.value };
    if (original === undefined || original === title) {
      delete next[seg.byte_start];
    } else {
      next[seg.byte_start] = title;
    }
    titleOverrides.value = next;
  }

  function reset() {
    markers.value = [];
    suppressed.value = [];
    titleOverrides.value = {};
    scheduleRecompute();
  }

  /// "重新切分"按钮:丢弃 committed 数据,回 splitter 首次结果。
  async function reSplit() {
    if (uploadId.value === null || sourceKind.value !== 'committed') return;
    const id = uploadId.value;
    const token = ++requestToken;
    try {
      const fresh = await ipcListChapterSegments(id, [], []);
      if (token !== requestToken) return;
      source.value = fresh;
      sourceKind.value = 'fresh';
      await applyWorking(token);
    } catch (e: unknown) {
      if (token === requestToken) error.value = e instanceof Error ? e.message : String(e);
    }
  }

  function setSearchQuery(q: string) {
    searchQuery.value = q;
    currentHitIndex.value = 0;
  }

  function nextSearchHit(total: number) {
    if (total === 0) return;
    const safe = Math.min(currentHitIndex.value, total - 1);
    currentHitIndex.value = (safe + 1) % total;
  }

  function prevSearchHit(total: number) {
    if (total === 0) return;
    const safe = Math.min(currentHitIndex.value, total - 1);
    currentHitIndex.value = (safe - 1 + total) % total;
  }

  /// 把当前 workingChapters 落库到新 data_asset,返回新 data_asset_id。
  /// commit 后 source 同步成 workingChapters,edits 清空,sourceKind 升级 committed。
  async function commit(title: string): Promise<number> {
    if (uploadId.value === null) throw new Error('未加载 upload');
    const segs: ChapterInput[] = workingChapters.value.map((s) => ({
      title: s.title,
      byte_start: s.byte_start,
      byte_end: s.byte_end,
    }));
    try {
      const newDataAssetId = await ipcCommitDataAsset(uploadId.value, { title, chapters: segs });
      source.value = workingChapters.value.map((s) => ({ ...s }));
      sourceKind.value = 'committed';
      markers.value = [];
      suppressed.value = [];
      titleOverrides.value = {};
      return newDataAssetId;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    }
  }

  return {
    uploadId, rawText, filename,
    rawLines,
    workingChapters,
    markers, suppressed, titleOverrides,
    source, sourceKind,
    loading, error,
    searchQuery, currentHitIndex,
    committed, dirty,
    load,
    addMarker, removeMarker, removeChapter, updateTitle, reset, reSplit, commit,
    setSearchQuery, nextSearchHit, prevSearchHit,
  };
});
