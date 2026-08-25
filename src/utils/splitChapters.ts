import type { ChapterSegment } from '../ipc/types';

/// 转义正则特殊字符 —— 用作宽容搜索 regex 的字面量部分。
function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^\${()|[\]\\]/g, '\\\\/// zh-aware 字数简化版');
}

/// zh-aware 字数简化版:Chinese 全角字符 + 英文/数字连续段都计 1。
/// 真正的 word_count 后端按 byte range 切片时精算,这里只用于 UI 列表显示。
/// 不可见字符(空白 + ZWSP/BOM/WJ 等 format characters)正则 —— 用于在
/// 已知会被 strip 的前后缀上做宽松比较(如 raw 与 splitted segLines 一侧带
/// 不可见字符)。
const INVIS_PREFIX_RE = /^[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+/u;
const INVIS_SUFFIX_RE = /[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+$/u;
export function isVisuallyEmptyLine(line: string): boolean {
  return line.replace(INVIS_PREFIX_RE, '').replace(INVIS_SUFFIX_RE, '') === '';
}

export function countChapterChars(s: string): number {
  let n = 0;
  for (const ch of s) if (!/\s/.test(ch)) n++;
  return n;
}

/// 通过 chapter title 在原文里定位 body 起始行 (0-based)。
/// 容忍 title 自身和原文里的不可见字符 —— 后端 splitter 用 [\s\p{Cf}]* 匹配章节标题行,
/// 这些 invisibles 留在 rawText 里,可能让 plain indexOf 失败 (尤其 title 一侧被 strip 过)。
///
/// 返回 -1 表示 title 在 text 里完全没出现(理论不该发生,留作 sentinel)。
function bodyStartByTitle(text: string, title: string): number {
  const totalLines = text.split('\n').length;
  // 先 strip title 自带的 invisibles —— splitter 输出的 title 一般是干净的,
  // 但如果 store / IPC 通道某处意外带过来 1-2 个 \u200B,会导致 indexOf 整体失败。
  const cleanTitle = title
    .replace(/^[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+/u, '')
    .replace(/[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+$/u, '');
  if (!cleanTitle) return -1;
  // 1) 先尝试 plain indexOf(快路径:cleanTitle 是 rawText 的子串)。
  let titleIdx = text.indexOf(cleanTitle);
  if (titleIdx < 0) {
    // 2) 兜底:在 cleanTitle 前允许最多 8 个 whitespace / invisible 字符 ——
    //    rawText 标题行前可能有 \u3000、\u200B 等 leading invisibles。
    const re = new RegExp(`[\\\\s\\\\u{200B}\\\\u{200C}\\\\u{200D}\\\\u{FEFF}\\\\u{2060}]{0,8}${escapeRegExp(cleanTitle)}`, 'u');
    const m = re.exec(text);
    if (m) titleIdx = m.index + m[0].length - cleanTitle.length;
  }
  if (titleIdx < 0) return -1;
  const titleEnd = text.indexOf('\n', titleIdx);
  if (titleEnd < 0) return totalLines; // title 是最后一行,后面没 body
  // 标题行的 \n 算在 slice 里,再 -1 得到"标题行后第一行"的行号。
  return text.slice(0, titleEnd + 1).split('\n').length - 1;
}

/// 通过 chapter content 在原文里定位 body 起始行 (0-based)。
/// 作为 title 找不到时的兜底:content 可能很长 (上千字),中间任何字符差异
/// (全角空格 / 零宽 / BOM / \r\n vs \n) 都会让 indexOf 整段失败 —— 这正是
/// 之前 segs[0] (4824 字) 在 text 里 indexOf 返回 -1 但前 100 字能匹配的原因。
function bodyStartByContent(text: string, content: string): number {
  const idx = text.indexOf(content);
  if (idx < 0) return -1;
  return text.slice(0, idx).split('\n').length - 1;
}

/// 严格模式:只匹配章节标题正则(「第N[章回]xxx」)。
/// marker 落在 rawText 的章节标题行时,merge/合并后的 seg.content 不包含该行,
/// 这时 splitLine 是普通正文行;strict 在 rawLineAtMarker 上检测可以稳定识别标题。
function parseChapterTitleStrict(line: string): string | null {
  if (!line) return null;
  const m = line.match(/^[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]*第[零一二三四五六七八九十百千万亿0-9]+[章回][^\n]*$/u);
  if (!m) return null;
  return m[0].replace(/^[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+/u, '').replace(/[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+$/u, '');
}

/// 宽松模式:严格模式没命中,fallback 到「清理后非空的整行 → 标题候选」。
/// 用户在 UI 上点「章」= 显式表达「这一行起算新章节」,无论这一行 4 字还是 400 字
/// 都是同一意图 —— 只要清理后非空,直接用做新章节 title,而不是「（续）」后缀。
/// marker 行本身为空 / 仅 whitespace / 仅 invisibles 的退化场景由 split 里的
/// zero-word guard 兜底(避免产生没正文的'僵尸'章节)。
///
/// 返回清理后的 title(去掉首尾 invisible/whitespace);空串返回 null。
function parseChapterTitle(line: string): string | null {
  if (!line) return null;
  const strict = parseChapterTitleStrict(line);
  if (strict !== null) return strict;
  const cleaned = line.replace(/^[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+/u, '').replace(/[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+$/u, '');
  if (cleaned) return cleaned;
  return null;
}
/// 把 (startLine, endLine, title) 跟 markers 求交,得到"命中本段的 markers 列表"。
/// 严格在 body 开区间内 (startLine, endLine),避开 body 首行/标题行 —— 这俩已经是
/// 章节边界,marker 落上去要么产生空章节、要么没有意义。
function findHitsInRange(
  startLine: number,
  endLine: number,
  markerSet: Set<number>,
): number[] {
  return Array.from(markerSet)
    .filter((m) => m > startLine && m < endLine)
    .sort((a, b) => a - b);
}

/// 按 markers (0-based 行号) 把 segs 切开。
///
/// - 每个 marker 必须严格落在某个 segment body 的 (startLine, endLine) 开区间内,
///   落在 body 首行/末行/标题行/段外都会被滤掉(避免产生空章节或冗余切分)。
/// - 切点 = marker 行号;marker 所在行归入"新章节"(即下半部分)。
/// - 第一半继承原标题,后续半挂"“（续）”"后缀(U+201C/201D + FF08/FF09 + 7EED)。
///
/// 返回新数组(不修改入参)。rawText 必须与后端 splitter 看到的原文一致,
/// 用它来反查 seg.title 在源里的 body 起始行。
export function splitChaptersByMarkers(
  segs: readonly ChapterSegment[],
  markerKeys: readonly string[],
  rawText: string,
): ChapterSegment[] {
  if (markerKeys.length === 0 || !rawText) return segs.slice();
  const text = rawText;
  // 预计算一次,避免后面每次 text.split('\n') 重复扫全文。
  const rawLines = text.split('\n');
  const totalLines = rawLines.length;
  const markerSet = new Set<number>();
  for (const m of markerKeys) {
    const n = Number.parseInt(m, 10);
    if (Number.isFinite(n) && n >= 0 && n < totalLines) markerSet.add(n);
  }
  if (markerSet.size === 0) return segs.slice();

  // body start line:title 优先,content 兜底。
  const segBodyStart: number[] = [];
  for (const seg of segs) {
    let line = bodyStartByTitle(text, seg.title);
    if (line < 0) line = bodyStartByContent(text, seg.content);
    segBodyStart.push(line);
  }

  const result: ChapterSegment[] = [];
  for (let i = 0; i < segs.length; i++) {
    const seg = segs[i];
    const startLine = segBodyStart[i];
    if (startLine < 0) { result.push(seg); continue; }
    // endLine = 下一章的标题行 (0-based),开区间不含它。
    // 标题行本身就是章节边界,让 marker 落到标题行 = "在 chapter 之间",filter 自然排除,
    // 不会误算到上一章 body 末行 —— 上一章 body 的最后一行是 (endLine - 1),仍可正常切。
    // 最后一章没有"下一章",用 startLine + bodyLen 做等价的右开区间。
    const nextBodyStart = i + 1 < segs.length ? segBodyStart[i + 1] : -1;
    const endLine = nextBodyStart >= 0
      ? nextBodyStart - 1
      : startLine + seg.content.split('\n').length;

    const inside = findHitsInRange(startLine, endLine, markerSet);
    if (inside.length === 0) { result.push(seg); continue; }

    // segLines 行号 ≠ rawLines 行号(合并后章节标题不在 seg.content 中)。
    // 构造 rawLines[startLine..endLine] → segLines 索引的映射:
    // - raw 上的严格章节标题(如「第N章」)若已被合并时剥离,在表中为 -1。
    // - 其他 raw 行映射到连续的 segLines 索引。
    const rawToSegIdx = new Map<number, number>();
    {
      let segIdx = 0;
      for (let raw = startLine; raw <= endLine; raw++) {
        if (parseChapterTitleStrict(rawLines[raw] ?? "")) {
          rawToSegIdx.set(raw, -1);
        } else {
          rawToSegIdx.set(raw, segIdx);
          segIdx++;
        }
      }
    }

    const segLines = seg.content.split('\n');
    let partStart = 0;
    // lastPushedTitle:刚 push 的那段 title,用于多段切分时累加「（续）」。
    // —— 之前用 seg.title 在 partIdx>0 时会让第 2/N 段全部退化成「seg.title（续）」,
    // —— 看起来「（续）」只追加一次,但用户连续切两处非标题行,期望第 2 段标题变成
    // —— 「第一章第三世（续）」（续）」而不是再次「第一章第三世（续）」。
    let lastPushedTitle = seg.title;
    let nextPartTitle = seg.title;
    for (const mLine of inside) {
      const rawSegIdx = rawToSegIdx.get(mLine);
      let splitAt: number;
      let titleStripped = false;
      if (rawSegIdx === undefined || rawSegIdx < 0) {
        // marker 落在 raw 上的严格章节标题 —— 分两种情况:
        //   (a) seg.content 已在合并时剥离该标题行(real mergeSuppressed)
        //       —— 跳到下一行 body 作为 split 位置。
        //   (b) seg.content 里仍保留该标题行(合成 merge / 多章 source)
        //       —— 找到 segLines 中位置,常规 +1 跳过。
        const markerLine = rawLines[mLine] ?? "";
        let segMatch = segLines.indexOf(markerLine);
        if (segMatch < 0) {
          // 兼容 raw 与 segLines 一侧带不可见字符的差异(如 spaces/BOM)。
          const strippedMarker = markerLine.replace(INVIS_PREFIX_RE, "").replace(INVIS_SUFFIX_RE, "");
          if (strippedMarker) segMatch = segLines.indexOf(strippedMarker);
        }
        if (segMatch >= 0) {
          splitAt = segMatch;
          titleStripped = false;
        } else {
          titleStripped = true;
          splitAt = -1;
          for (let raw = mLine + 1; raw <= endLine; raw++) {
            const v = rawToSegIdx.get(raw);
            if (v !== undefined && v >= 0) { splitAt = v; break; }
          }
          if (splitAt < 0) continue;
        }
      } else {
        splitAt = rawSegIdx;
        titleStripped = false;
      }
      if (splitAt <= 0 || splitAt >= segLines.length) continue;
      const splitLine = segLines[splitAt] ?? '';
      // 合并 content 里没有标题行(mergeSuppressed 只拼 body 不拼 title),
      // 所以 segLines 看不到 chapter title 行 —— 必须 fallback 到 rawText 在 marker 行上的原内容。
      const rawLineAtMarker = rawLines[mLine] ?? '';
      // 优先级:
      //   1) strict 在 rawLineAtMarker 上检测(稳定识别合并后还能定位章节标题)
      //   2) strict 在 splitLine 上检测(seg.content 里就含标题行的罕见情况)
      //   3) 宽松在 splitLine 上检测(短句正文行作为新章节标题)
      //   4) 宽松在 rawLineAtMarker 上检测(兜底,实际基本走不到这步)
            const detected = parseChapterTitleStrict(rawLineAtMarker)
        ?? parseChapterTitleStrict(splitLine)
        ?? parseChapterTitle(splitLine)
        ?? parseChapterTitle(rawLineAtMarker);
      // 空 marker 行(纯空 / 纯 whitespace / 纯 invisibles) → 整个 split 跳过。
      // UI 层 lineMarker 已经不渲染按钮,splitter 实际收不到 —— 作为兜底,
      // 不 push 任何东西(partStart / lastPushedTitle / nextPartTitle 都不动)。
      if (detected === null) continue;
      // push 当前段(partStart..splitAt)
      const part = segLines.slice(partStart, splitAt).join('\n');
      // 防御:part 是纯 whitespace/invisible 也不切 ——
      // 否则 marker 紧贴 chapter body 起点时会把 chapter 2 切空,体感是 bug。
      if (countChapterChars(part) === 0) continue;
      // 上半段照常 push;下半段的 zombie 防御(字数 = 0 不 push)由 lastClean 那段统一处理,
      // 这里不重复检查 splitAt,否则会把上半段也丢掉("咚 咚 咚 ！" 这种 marker
      // 行 == 最后一行的场景,期望 out[0].content 只剩上半段)。
      result.push({
        title: nextPartTitle,
        content: part,
        word_count: countChapterChars(part),
      });
      lastPushedTitle = nextPartTitle;
      // marker 行有内容 → 整行(清理 invisibles)作为新章节标题。
      nextPartTitle = detected;
      // titleStripped:raw 上的标题已被合并剥离,segLines[splitAt] 已是新章节的
      // 第一行 body,不再 +1 跳过;否则 marker 行落在 segLines 内,需跳过此行。
      partStart = titleStripped ? splitAt : splitAt + 1;
    }
    // 去掉合并内容末尾的换行/空白 —— 内容在 splitter 看来不应带 trailing \n,
    // 避免给前端渲染或后续 round-trip 制造噪音(测试期望也不带)。
    const lastRaw = segLines.slice(partStart).join('\n');
    const lastClean = lastRaw.replace(/[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+$/u, '');
    // 防御:不要 push word_count=0 的'僵尸'章节(marker 紧贴 body 末尾且 detect 到下一段标题时,
    // 上半段全为 whitespace/invisible,强 push 会留下一行带标题却没正文的章节)。
    if (countChapterChars(lastClean) > 0) {
      result.push({
        title: nextPartTitle,
        content: lastClean,
        word_count: countChapterChars(lastClean),
      });
    }
  }
  return result;
}


/// 诊断工具:返回 segBodyStart 数组 + 每个 marker 命中的 chapter 索引(若有)。
/// 用于 UI 调试 —— 调用方自行 console.log。
export function diagnoseSplit(
  segs: readonly ChapterSegment[],
  markerKeys: readonly string[],
  rawText: string,
): {
  totalLines: number;
  bodyStarts: number[];
  negativeIndices: number[];
  hits: Array<{ chapterIndex: number; title: string; startLine: number; endLine: number; splitAt: number; bodyLen: number }>;
} {
  const empty = { totalLines: 0, bodyStarts: [] as number[], negativeIndices: [] as number[], hits: [] as Array<{ chapterIndex: number; title: string; startLine: number; endLine: number; splitAt: number; bodyLen: number }> };
  if (!rawText) return empty;
  const text = rawText;
  const totalLines = text.split('\n').length;
  const bodyStarts: number[] = [];
  for (const seg of segs) {
    let line = bodyStartByTitle(text, seg.title);
    if (line < 0) line = bodyStartByContent(text, seg.content);
    bodyStarts.push(line);
  }
  const negativeIndices = bodyStarts.map((v, i) => v < 0 ? i : -1).filter((i) => i >= 0);
  const markerSet = new Set<number>();
  for (const m of markerKeys) {
    const n = Number.parseInt(m, 10);
    if (Number.isFinite(n) && n >= 0 && n < totalLines) markerSet.add(n);
  }
  const hits: Array<{ chapterIndex: number; title: string; startLine: number; endLine: number; splitAt: number; bodyLen: number }> = [];
  for (let i = 0; i < segs.length; i++) {
    const start = bodyStarts[i];
    if (start < 0) continue;
    // 同 splitChaptersByMarkers:endLine 是下一章标题行。
    const nextBodyStart = i + 1 < segs.length ? bodyStarts[i + 1] : -1;
    const end = nextBodyStart >= 0
      ? nextBodyStart - 1
      : start + segs[i]!.content.split('\n').length;
    for (const m of markerSet) {
      if (m > start && m < end) {
        const segLines = segs[i]!.content.split('\n');
        hits.push({
          chapterIndex: i,
          title: segs[i]!.title,
          startLine: start,
          endLine: end,
          splitAt: m - start,
          bodyLen: segLines.length,
        });
      }
    }
  }
  return { totalLines, bodyStarts, negativeIndices, hits };
}