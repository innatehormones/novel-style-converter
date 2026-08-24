import { defineStore } from 'pinia';
import { useDebounceFn } from '@vueuse/core';
import { computed, ref } from 'vue';
import type { ChapterSegment, ChapterInput } from '../ipc/types';
import {
  commitDataAsset as ipcCommitDataAsset,
  findDataAssetByUpload as ipcFindDataAssetByUpload,
  getUploadText as ipcGetUploadText,
  getUpload as ipcGetUpload,
  listChapterSegments as ipcListChapterSegments,
  listCommittedSegments as ipcListCommittedSegments,
} from '../ipc/commands';
import { splitChaptersByMarkers } from '../utils/splitChapters';

type SourceKind = 'committed' | 'fresh';

interface RawLine { line: number; text: string; }

function computeLines(text: string): RawLine[] {
  return text.split('\n').map((t, i) => ({ line: i, text: t }));
}

/// 用 content 作为稳定 key:title 是可编辑的(applyTitleOverrides 会改),
/// content 是 splitter 输出的原始内容,不会被编辑(章节正文不会改)。
/// 这样 suppressed / titleOverrides / segLineMap 在 title 编辑后仍能查到。
function segmentKey(seg: ChapterSegment): string {
  return seg.content;
}

export const useChaptersStore = defineStore('chapters', () => {
  const uploadId = ref<number | null>(null);
  const rawText = ref<string>('');
  const filename = ref<string>('');

  const rawLines = computed<RawLine[]>(() => computeLines(rawText.value));

  const source = ref<ChapterSegment[]>([]);
  /// segmentKey -> 该段在原文里的起始行号。每次 applyWorking 重算。
  const segLineMap = ref<Map<string, number>>(new Map());
  const sourceKind = ref<SourceKind | null>(null);
  const markers = ref<string[]>([]);
  const suppressed = ref<string[]>([]);
  const titleOverrides = ref<Record<string, string>>({});

  const workingChapters = ref<ChapterSegment[]>([]);

  const loading = ref(false);
  const error = ref<string | null>(null);


  let requestToken = 0;

  const committed = computed(() => sourceKind.value === 'committed');

  const dirty = computed(() => {
    if (markers.value.length > 0 || suppressed.value.length > 0) return true;
    for (const k in titleOverrides.value) {
      const seg = source.value.find((s) => segmentKey(s) === k);
      if (!seg || titleOverrides.value[k] !== seg.title) return true;
    }
    return false;
  });

  async function applyWorking(token: number) {
    if (uploadId.value === null) return;
    const id = uploadId.value;
    // splitter 后端跑全文;markers/suppressed 都是前端 UI 状态,不传给后端。
    let segs: ChapterSegment[] = source.value.length > 0
      ? source.value
      : await ipcListChapterSegments(id);
    if (token !== requestToken) return;

    // 先按 markers 把 source 按行切开(原文行号),再看 suppressed 合并。
    // 否则用户在合并后重新加 marker 想复原章节边界,split 永远不生效,左侧不会变多。
    // segs -> splitByMarkers -> mergeSuppressed -> applyTitleOverrides。
    if (markers.value.length > 0 && rawText.value) {
      segs = splitChaptersByMarkers(segs, markers.value, rawText.value);
    }
    if (token !== requestToken) return;

    // 计算每段在原文里的起始行号,给"点击章节跳到右侧原文"用。
    segLineMap.value = computeLineMap(rawText.value, segs);

    // suppressed = "并入上一章":把该段 content/word_count 追加到上一段末尾,丢弃该段。
    if (suppressed.value.length > 0) {
      const supSet = new Set(suppressed.value);
      segs = mergeSuppressed(segs, supSet);
    }
    workingChapters.value = applyTitleOverrides(segs);
  }

  /// 从前向后 indexOf,避免同一 content 误匹配到后面的副本。
  /// map 里记录的是每个章节 (segmentKey -> 原文起始行号),给"点击章节跳转到右侧原文"用。
  ///
  /// 行号算法:content/title 第一字符所在的行号 = 该字符之前的完整行数。
  /// 实现:text.slice(0, idx) 包含 idx 之前的'\\n',split('\\n').length - 1 就是行号。
  /// 旧实现 text.slice(0, lastNl) 把那一个'\\n'吃掉了,得到的是**前一行的行号**,
  /// 用户点"第二章今世只想生孩子"会跳到上一章末尾 rawLines[N-1]("紧接着..." 等),
  /// 而不是标题自身 rawLines[N]("第二章今世只想生孩子")。
  ///
  /// 优先 content(content 是 splitter 输出的原始正文,merge/split 后会被改,
  /// 但 seq 顺序 + cursor 限制保证不回头匹配)。
  /// content 找不到时(merge 后 content 改了)再 fallback 到 title。
  function computeLineMap(text: string, segs: ChapterSegment[]): Map<string, number> {
    const map = new Map<string, number>();
    if (!text) return map;
    let cursor = 0;
    for (const s of segs) {
      let line = -1;
      const idxContent = text.indexOf(s.content, cursor);
      if (idxContent >= 0) {
        line = idxContent === 0 ? 0 : text.slice(0, idxContent).split('\n').length - 1;
        cursor = idxContent + s.content.length;
      } else {
        const cleanTitle = (s.title ?? '')
          .replace(/^[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+/u, '')
          .replace(/[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+$/u, '');
        if (cleanTitle) {
          const idxTitle = text.indexOf(cleanTitle);
          if (idxTitle >= 0) {
            line = idxTitle === 0 ? 0 : text.slice(0, idxTitle).split('\n').length - 1;
          }
        }
      }
      if (line >= 0) map.set(segmentKey(s), line);
    }
    return map;
  }

  /// 从前向后 indexOf,避免同一 content 误匹配到后面的副本。
  /// map 里记录的是"标题行号"——idx 是 content 第一个字符的位置,
  /// 往前找最近的换行,该换行之前的换行数就是标题所在行。
  /// 第一章被 suppress 时直接丢弃(没有"上一章"可并)。
  function mergeSuppressed(segs: ChapterSegment[], supSet: Set<string>): ChapterSegment[] {
    const out: ChapterSegment[] = [];
    for (const s of segs) {
      if (!supSet.has(segmentKey(s))) { out.push(s); continue; }
      if (out.length === 0) continue;
      const prev = out[out.length - 1];
      // Rust splitter trim() 章节 body —— content 不带首尾换行。
      // 拼接相邻章节时必须插一个换行,否则 segLines 行号会和 rawText
      // 失去 1-1 对应,后续 splitChaptersByMarkers 会把章节 2 的首行
      // body 错位切到上一章里、字数错乱。
      const NL = String.fromCharCode(10);
      const needsJoiner = !prev.content.endsWith(NL) && !s.content.startsWith(NL);
      out[out.length - 1] = {
        title: prev.title,
        content: prev.content + (needsJoiner ? NL : "") + s.content,
        word_count: prev.word_count + s.word_count,
      };
    }
    return out;
  }

  function applyTitleOverrides(segs: ChapterSegment[]): ChapterSegment[] {
    if (!segs) return [];
    const overrides = titleOverrides.value;
    return segs.map((s) => {
      const t = overrides[segmentKey(s)];
      const title = t !== undefined && t !== s.title ? t : s.title;
      return { title, content: s.content, word_count: s.word_count };
    });
  }

  async function recompute() {
    const token = ++requestToken;
    await applyWorking(token);
  }

  /// 200ms 防抖 — vueuse useDebounceFn 自动随 store 作用域销毁清理。
  const debouncedRecompute = useDebounceFn(() => { void recompute(); }, 200);

  async function load(id: number) {
    uploadId.value = id;
    workingChapters.value = [];
    source.value = [];
    sourceKind.value = null;
    rawText.value = '';
    filename.value = '';
    markers.value = [];
    suppressed.value = [];
    titleOverrides.value = {};
    loading.value = true;
    error.value = null;
    ++requestToken;
    const token = requestToken;
    try {
      // 元数据 + 原文 + 派生 data_asset 列表可并行。
      // 注意:listCommittedSegments 接收的是 data_asset_id 而不是 upload_id,
      // 之前误用 uploadId 调用,会撞到 id 相同的别的 data_asset 拿到别人家的章节。
      const [text, meta, dataAssetIds] = await Promise.all([
        ipcGetUploadText(id),
        ipcGetUpload(id).catch(() => null),
        ipcFindDataAssetByUpload(id).catch(() => [] as number[]),
      ]);
      if (token !== requestToken) return;
      rawText.value = text;
      filename.value = meta?.filename ?? '';
      let segs: ChapterSegment[];
      let kind: SourceKind;
      const ownedDataAssetId = dataAssetIds[0] ?? null;
      if (ownedDataAssetId !== null) {
        // 本 upload 派生过 data_asset,取最近一个的已提交章节作起始状态
        segs = await ipcListCommittedSegments(ownedDataAssetId);
        kind = 'committed';
      } else {
        segs = await ipcListChapterSegments(id);
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

  function addMarker(key: string) {
    if (markers.value.includes(key)) return;
    // 用户在已 suppress 章节的标题行点"章" = 想"复活"该章节。
    // 此前的 `suppressed.value.includes(key)` 永远 false:
    //   key 是行号(0-based),suppressed 是 seg.content —— 类型层就对不上。
    // 同时 splitChaptersByMarkers 在边界行 (m == start) 主动丢弃 marker,
    // 即使 marker 落到已 merge 章节的标题行,如果不 un-suppress,该章节也不会出现在左侧。
    // 这里走 line → source seg → segmentKey 的反查:命中 suppressed 后从该数组移除。
    const suppressedSeg = findSuppressedSegAtLine(key);
    if (suppressedSeg) {
      const segKey = segmentKey(suppressedSeg);
      suppressed.value = suppressed.value.filter((p) => p !== segKey);
    }
    markers.value = [...markers.value, key].sort();
    debouncedRecompute();
  }

  /// 反查"该行是不是 source 里某个已 suppress 章节的标题行"。
  /// 用于 addMarker 在 marker 命中章节标题时联动 un-suppress。
  /// rawText 没有 / 行号越界 / 该行不是任何 source seg 的标题 / 命中的 seg 未被 suppress —— 都返回 null。
  /// 不可见字符裁剪与 splitChapters.bodyStartByTitle 一致,保证 line 端/title 端两侧一致。
  function findSuppressedSegAtLine(lineKey: string): ChapterSegment | null {
    if (!rawText.value) return null;
    const lineIdx = Number.parseInt(lineKey, 10);
    if (!Number.isFinite(lineIdx) || lineIdx < 0) return null;
    const lines = rawText.value.split('\n');
    if (lineIdx >= lines.length) return null;
    const lineContent = lines[lineIdx] ?? '';
    const clean = lineContent
      .replace(/^[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+/u, '')
      .replace(/[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+$/u, '');
    if (!clean) return null;
    const seg = source.value.find((s) => s.title === clean);
    if (!seg) return null;
    return suppressed.value.includes(segmentKey(seg)) ? seg : null;
  }

  function removeMarker(key: string) {
    if (!markers.value.includes(key)) return;
    markers.value = markers.value.filter((m) => m !== key);
    debouncedRecompute();
  }

  function removeChapter(idx: number) {
    const seg = workingChapters.value[idx];
    if (!seg || idx === 0) return;
    const k = segmentKey(seg);
    if (!suppressed.value.includes(k)) {
      suppressed.value = [...suppressed.value, k].sort();
      debouncedRecompute();
    }
  }

  function updateTitle(idx: number, title: string) {
    const seg = workingChapters.value[idx];
    if (!seg) return;
    seg.title = title;
    const k = segmentKey(seg);
    const original = source.value.find((s) => segmentKey(s) === k)?.title;
    const next = { ...titleOverrides.value };
    if (original === undefined || original === title) {
      delete next[k];
    } else {
      next[k] = title;
    }
    titleOverrides.value = next;
  }

  function reset() {
    markers.value = [];
    suppressed.value = [];
    titleOverrides.value = {};
    debouncedRecompute();
  }

  /// 离开 parse 页时调用:清掉 rawText/source/workingChapters 等大对象,避免 pinia store
  /// 持有旧小说数据驻内存(load 覆盖赋值会 GC,但显式 unload 更明确,也避免 watch 防抖悬挂)。
  function unload() {
    uploadId.value = null;
    rawText.value = '';
    filename.value = '';
    source.value = [];
    sourceKind.value = null;
    markers.value = [];
    suppressed.value = [];
    titleOverrides.value = {};
    workingChapters.value = [];
    segLineMap.value = new Map();
    error.value = null;
    loading.value = false;
  }

  async function reSplit() {
    if (uploadId.value === null || sourceKind.value !== 'committed') return;
    const id = uploadId.value;
    const token = ++requestToken;
    try {
      const fresh = await ipcListChapterSegments(id);
      if (token !== requestToken) return;
      source.value = fresh;
      sourceKind.value = 'fresh';
      await applyWorking(token);
    } catch (e: unknown) {
      if (token === requestToken) error.value = e instanceof Error ? e.message : String(e);
    }
  }

  /// 拿到一段章节在原文里的起始行号(用于点击章节跳转到右侧原文)。
  function startLineOf(seg: ChapterSegment): number {
    return segLineMap.value.get(segmentKey(seg)) ?? -1;
  }

  async function commit(title: string): Promise<number> {
    if (uploadId.value === null) throw new Error('no upload loaded');
    const segs: ChapterInput[] = workingChapters.value.map((s) => ({
      title: s.title,
      content: s.content,
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
    segLineMap,
    loading, error,
    committed, dirty,
    load,
    addMarker, removeMarker, removeChapter, updateTitle, reset, reSplit, commit,
    startLineOf,
    unload,
  };
});
