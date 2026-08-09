/// 展示层格式化工具。统一前后端渲染口径:
///
/// - `formatWordCount` 在 Library.vue 和 parse.vue 之前各写一份,产物不一致
///   (list 写 "1.2 万字"、parse 写 "12,000 字")。集中到这里,选 parse 的版本
///   (千分位 + "字"),因为字数对作者来说要精确,不能 1 位小数截掉。
/// - `formatSize` / `formatTime` 之前只 Library.vue 用;Transform 页等后续要展示
///   字节/时间时直接复用,不用再拷一遍。
///
/// 全部 pure 函数,不依赖 Vue / Tauri / DOM;放在 `utils/` 而不是 `composables/`
/// 因为 composables 暗示会用到响应式 ref(参见现有 `useChapterSearch.ts`)。

const ZH_LOCALE = 'zh-Hans-CN';

export function formatSize(b: number): string {
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${(b / 1024 / 1024).toFixed(2)} MB`;
}

/// 与后端 text::word::count 一致: 字数 = 除空白外的所有字符。
/// 包含汉字、字母、数字、标点符号(ASCII 标点 + CJK 标点)。
/// 与 Word / WPS / 网文平台 / AI 输出的字数概念一致。
export function countWords(s: string): number {
  if (s.length === 0) return 0;
  let n = 0;
  for (const c of s) {
    if (!/\s/.test(c)) n += 1;
  }
  return n;
}


/// 千分位 + "字"。`!Number.isFinite(n) || n < 0` 时显示 "?",避免负数 / NaN
/// 渲染成 "NaN 字" 之类的调试串。零值显示 "0 字"。
export function formatWordCount(n: number): string {
  if (!Number.isFinite(n) || n < 0) return '?';
  return `${n.toLocaleString(ZH_LOCALE)} 字`;
}

/// ISO-8601 → "YYYY-MM-DD HH:mm"。后端全部返回 RFC3339,这里只切前 16 字符。
export function formatTime(iso: string): string {
  return iso.replace('T', ' ').slice(0, 16);
}