import { describe, expect, it } from 'vitest';
import { splitChaptersByMarkers, countChapterChars, diagnoseSplit } from '../utils/splitChapters';
import type { ChapterSegment } from '../ipc/types';

function seg(title: string, body: string): ChapterSegment {
  return { title, content: body, word_count: countChapterChars(body) };
}
const mkSeg = seg;

describe('splitChaptersByMarkers', () => {
  it('empty markers returns shallow copy', () => {
    const segs = [seg('A', 'aa'), seg('B', 'bb')];
    const text = 'T1\naa\nT2\nbb\n';
    const out = splitChaptersByMarkers(segs, [], text);
    expect(out).toHaveLength(2);
    expect(out).not.toBe(segs);
  });

  it('marker in second body line splits body and assigns marker line to lower half', () => {
    const text = 'T1\nbody line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests\nbody line 3 placeholder for chapter 1 split tests\nT2\nbody line 1 placeholder for chapter 2 split tests\nbody line 2 placeholder for chapter 2 split tests\n';
    const segs = [seg('T1', 'body line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests\nbody line 3 placeholder for chapter 1 split tests'), seg('T2', 'body line 1 placeholder for chapter 2 split tests\nbody line 2 placeholder for chapter 2 split tests')];
    const out = splitChaptersByMarkers(segs, ['2'], text);
    expect(out).toHaveLength(3);
    expect(out[0]?.title).toBe('T1');
    expect(out[0]?.content).toBe('body line 1 placeholder for chapter 1 split tests');
    expect(out[1]?.title).toBe('T1\u201C\uFF08\u7EED\uFF09\u201D');
    expect(out[1]?.content).toBe('body line 2 placeholder for chapter 1 split tests\nbody line 3 placeholder for chapter 1 split tests');
    expect(out[2]?.title).toBe('T2');
  });

  it('marker on first body line does not split', () => {
    const text = 'T1\nbody line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests\nT2\nbody line 1 placeholder for chapter 2 split tests\n';
    const segs = [seg('T1', 'body line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests'), seg('T2', 'body line 1 placeholder for chapter 2 split tests')];
    const out = splitChaptersByMarkers(segs, ['1'], text);
    expect(out).toHaveLength(2);
    expect(out[0]?.content).toBe('body line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests');
  });

  it('marker on last body line splits, marker line in lower half', () => {
    const text = 'T1\nbody line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests\nT2\nbody line 1 placeholder for chapter 2 split tests\n';
    const segs = [seg('T1', 'body line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests'), seg('T2', 'body line 1 placeholder for chapter 2 split tests')];
    const out = splitChaptersByMarkers(segs, ['2'], text);
    expect(out).toHaveLength(3);
    expect(out[0]?.content).toBe('body line 1 placeholder for chapter 1 split tests');
    expect(out[1]?.content).toBe('body line 2 placeholder for chapter 1 split tests');
  });

  it('marker on title line does not split', () => {
    const text = 'T1\nbody line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests\nT2\nbody line 1 placeholder for chapter 2 split tests\n';
    const segs = [seg('T1', 'body line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests'), seg('T2', 'body line 1 placeholder for chapter 2 split tests')];
    const out = splitChaptersByMarkers(segs, ['3'], text);
    expect(out).toHaveLength(2);
  });

  it('short chapter: marker between body and next title does not split', () => {
    const text = 'T1\nbody line 1 placeholder for chapter 1 split tests\nT2\nbody line 1 placeholder for chapter 2 split tests\nbody line 2 placeholder for chapter 2 split tests\nT3\nbody line 1 placeholder for chapter 3 split tests\n';
    const segs = [seg('T1', 'body line 1 placeholder for chapter 1 split tests'), seg('T2', 'body line 1 placeholder for chapter 2 split tests\nbody line 2 placeholder for chapter 2 split tests'), seg('T3', 'body line 1 placeholder for chapter 3 split tests')];
    const out = splitChaptersByMarkers(segs, ['2'], text);
    expect(out).toHaveLength(3);
    expect(out[1]?.content).toBe('body line 1 placeholder for chapter 2 split tests\nbody line 2 placeholder for chapter 2 split tests');
  });

  it('multiple markers in same chapter produce three parts', () => {
    const text = 'T1\nbody line 1 for multi-split test\nbody line 2 for multi-split test\nbody line 3 for multi-split test\nbody line 4 for multi-split test\nbody line 5 for multi-split test\n';
    const segs = [seg('T1', 'body line 1 for multi-split test\nbody line 2 for multi-split test\nbody line 3 for multi-split test\nbody line 4 for multi-split test\nbody line 5 for multi-split test')];
    const out = splitChaptersByMarkers(segs, ['2', '4'], text);
    expect(out).toHaveLength(3);
    expect(out[0]?.content).toBe('body line 1 for multi-split test');
    expect(out[1]?.content).toBe('body line 2 for multi-split test\nbody line 3 for multi-split test');
    expect(out[2]?.content).toBe('body line 4 for multi-split test\nbody line 5 for multi-split test');
  });

  it('first chapter body starting at idx = 0', () => {
    const text = 'body line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests\nbody line 3 placeholder for chapter 1 split tests\n';
    const segs = [seg('(no title)', 'body line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests\nbody line 3 placeholder for chapter 1 split tests')];
    const out = splitChaptersByMarkers(segs, ['1'], text);
    expect(out).toHaveLength(2);
    expect(out[0]?.content).toBe('body line 1 placeholder for chapter 1 split tests');
    expect(out[1]?.content).toBe('body line 2 placeholder for chapter 1 split tests\nbody line 3 placeholder for chapter 1 split tests');
  });

  it('marker out of range is ignored', () => {
    const text = 'T1\nbody line 1 placeholder for chapter 1 split tests\n';
    const segs = [seg('T1', 'body line 1 placeholder for chapter 1 split tests')];
    const out = splitChaptersByMarkers(segs, ['999'], text);
    expect(out).toHaveLength(1);
  });

  it('invalid marker strings are ignored', () => {
    const text = 'T1\nbody line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests\n';
    const segs = [seg('T1', 'body line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests')];
    const out = splitChaptersByMarkers(segs, ['abc', ''], text);
    expect(out).toHaveLength(1);
  });

  it('marker only in second chapter body splits only that one', () => {
    const text = 'T1\nbody line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests\nT2\nbody line 1 placeholder for chapter 2 split tests\nbody line 2 placeholder for chapter 2 split tests\nbody line 3 placeholder for chapter 2 split tests\nT3\nbody line 1 placeholder for chapter 3 split tests\n';
    const segs = [
      seg('T1', 'body line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests'),
      seg('T2', 'body line 1 placeholder for chapter 2 split tests\nbody line 2 placeholder for chapter 2 split tests\nbody line 3 placeholder for chapter 2 split tests'),
      seg('T3', 'body line 1 placeholder for chapter 3 split tests'),
    ];
    const out = splitChaptersByMarkers(segs, ['5'], text);
    expect(out).toHaveLength(4);
    expect(out[0]?.content).toBe('body line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests');
    expect(out[1]?.content).toBe('body line 1 placeholder for chapter 2 split tests');
    expect(out[2]?.content).toBe('body line 2 placeholder for chapter 2 split tests\nbody line 3 placeholder for chapter 2 split tests');
    expect(out[3]?.content).toBe('body line 1 placeholder for chapter 3 split tests');
  });

  it('first chapter body starts at idx = 0, marker on first body line is filtered', () => {
    const text = 'body line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests\nbody line 3 placeholder for chapter 1 split tests';
    const segs = [seg('(no title)', 'body line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests\nbody line 3 placeholder for chapter 1 split tests')];
    const out = splitChaptersByMarkers(segs, ['0'], text);
    expect(out).toHaveLength(1);
  });
});

describe('countChapterChars', () => {
  it('whitespace is not counted', () => {
    expect(countChapterChars('  a b\nc')).toBe(3);
  });
  it('empty string returns 0', () => {
    expect(countChapterChars('')).toBe(0);
  });
  it('chinese + english + digits', () => {
    expect(countChapterChars('你好 world 123')).toBe(10);
  });
});

describe('diagnoseSplit', () => {
  it('uses title to locate body start when content has hidden character mismatches', () => {
    // 复现用户场景:splitter 的 .trim() 跟原文不完全一致(content 全文 indexOf 失败)。
    // 这里用零宽空格 \u200B 模拟 splitter 把零宽字符吃掉的边角情况。
    const only = mkSeg('第1章：我是好人', '初秋\u200B，大雨磅礴。许青啪嗒');
    const text = '内容简介\n\n\n第1章：我是好人\n初秋，大雨磅礴。许青啪嗒\n第2章：这是个误会\n';
    const diag = diagnoseSplit([only], [], text);
    expect(diag.bodyStarts[0]).toBeGreaterThan(0);
    expect(diag.negativeIndices).toEqual([]);
  });

  it('bodyStart is the line AFTER the title line, not the title line itself', () => {
    const text = 'AAAA\nBBBB\n第1章：标题\n正文第一行\n正文第二行\n';
    const s = seg('第1章：标题', '正文第一行\n正文第二行');
    const diag = diagnoseSplit([s], [], text);
    // title 在第 2 行(0-based),body 应在第 3 行。
    expect(diag.bodyStarts[0]).toBe(3);
  });

  it('falls back to content indexOf when title is not found', () => {
    const text = 'BODY CONTENT HERE\nMORE BODY\n';
    const s = { title: 'UNKNOWN_TITLE', content: 'BODY CONTENT HERE\nMORE BODY', word_count: 8 };
    const diag = diagnoseSplit([s], [], text);
    expect(diag.bodyStarts[0]).toBe(0);
  });

  it('reports negativeIndices for chapters whose body could not be located', () => {
    const text = 'TITLE_X\nBODY_X\nTITLE_Y\nBODY_Y\n';
    const segs = [
      seg('TITLE_X', 'BODY_X'),
      { title: 'NOT_IN_TEXT', content: 'NEVER_APPEARS', word_count: 0 } as ChapterSegment,
      seg('TITLE_Y', 'BODY_Y'),
    ];
    const diag = diagnoseSplit(segs, [], text);
    expect(diag.bodyStarts[0]).toBe(1);
    expect(diag.bodyStarts[1]).toBe(-1);
    expect(diag.negativeIndices).toEqual([1]);
    expect(diag.bodyStarts[2]).toBe(3);
  });

  it('hits list is empty when marker lands on a title line', () => {
    const text = 'T1\nbody line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests\nT2\nbody line 1 placeholder for chapter 2 split tests\n';
    const segs = [seg('T1', 'body line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests'), seg('T2', 'body line 1 placeholder for chapter 2 split tests')];
    // line 3 = T2 标题行
    const diag = diagnoseSplit(segs, ['3'], text);
    expect(diag.hits).toEqual([]);
  });

  it('hits list contains one entry per (chapter, marker) pair inside the chapter body', () => {
    const text = 'T1\nbody line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests\nbody line 3 placeholder for chapter 1 split tests\nbody line 4 placeholder for chapter 1 split tests\nT2\nbody line 1 placeholder for chapter 2 split tests\n';
    const segs = [seg('T1', 'body line 1 placeholder for chapter 1 split tests\nbody line 2 placeholder for chapter 1 split tests\nbody line 3 placeholder for chapter 1 split tests\nbody line 4 placeholder for chapter 1 split tests'), seg('T2', 'body line 1 placeholder for chapter 2 split tests')];
    // 行号:0=T1, 1=body line 1 placeholder for chapter 1 split tests, 2=body line 2 placeholder for chapter 1 split tests, 3=body line 3 placeholder for chapter 1 split tests, 4=body line 4 placeholder for chapter 1 split tests, 5=T2, 6=body line 1 placeholder for chapter 2 split tests
    // marker 2 (body line 2 placeholder for chapter 1 split tests) 和 4 (body line 4 placeholder for chapter 1 split tests) 都在 T1 body 内。
    const diag = diagnoseSplit(segs, ['2', '4'], text);
    expect(diag.hits).toHaveLength(2);
    expect(diag.hits[0]?.splitAt).toBe(1);
    expect(diag.hits[1]?.splitAt).toBe(3);
    expect(diag.hits[0]?.bodyLen).toBe(4);
  });
});
describe('marker at chapter-title line', () => {
  it('recreates chapter 2 when marker lands on its title line within merged content', () => {
    const raw = "第1章：我是好人\nbody1\n第2章：这是个误会\nbody2\n第3章：他们都已经成为历史\nbody3\n";
    const mergedSegs = [{
      title: "第1章：我是好人",
      content: "body1\n第2章：这是个误会\nbody2\n第3章：他们都已经成为历史\nbody3\n",
      word_count: 100,
    }];
    const out = splitChaptersByMarkers(mergedSegs, ["2"], raw);
    expect(out).toHaveLength(2);
    expect(out[0]?.title).toBe("第1章：我是好人");
    expect(out[1]?.title).toBe("第2章：这是个误会");
    expect(out[0]?.content).toBe("body1");
    expect(out[1]?.content).toBe("body2\n第3章：他们都已经成为历史\nbody3");
  });


  it("detects chapter title with leading ZWSP for split", () => {
    const raw = "第1章：我是好人\nbody1\n\u200B第2章：这是个误会\nbody2\n";
    const mergedSegs = [{
      title: "第1章：我是好人",
      content: "body1\n\u200B第2章：这是个误会\nbody2",
      word_count: 100,
    }];
    // raw 行号:0=第1章, 1=body1, 2=\u200B第2章, 3=body2, 4=空(尾换行)。
    // marker 必须落在 (bodyStart, endLine) 开区间内 —— bodyStart=1, endLine=4,
    // 所以 marker 必须是 2 或 3。选 2 命中 ZWSP 前导的章节标题。
    const out = splitChaptersByMarkers(mergedSegs, ["2"], raw);
    expect(out).toHaveLength(2);
    expect(out[1]?.title).toBe("第2章：这是个误会");
  });

  it("multiple chapter-title markers produce multiple proper chapters", () => {
    const raw = "第1章：我是好人\nbody1\n第2章：这是个误会\nbody2\n第3章：他们都已经成为历史\nbody3\n";
    const mergedSegs = [{
      title: "第1章：我是好人",
      content: "body1\n第2章：这是个误会\nbody2\n第3章：他们都已经成为历史\nbody3",
      word_count: 100,
    }];
    const out = splitChaptersByMarkers(mergedSegs, ["2", "4"], raw);
    expect(out).toHaveLength(3);
    expect(out.map((s) => s.title)).toEqual(["第1章：我是好人", "第2章：这是个误会", "第3章：他们都已经成为历史"]);
  });

  it("recreates chapter 2 even when rawText has invisibles around titles", () => {
    const raw = "\u200B第1章：我是好人\nbody1\n\u3000第2章：这是个误会\nbody2\n";
    const mergedSegs = [{
      title: "第1章：我是好人",
      content: "body1\n\u3000第2章：这是个误会\nbody2",
      word_count: 100,
    }];
    // raw 行号:0=\u200B第1章, 1=body1, 2=\u3000第2章, 3=body2, 4=空(尾换行)。
    // marker 必须落在 (bodyStart, endLine) 开区间内 —— bodyStart=1, endLine=4,
    // 所以 marker 必须是 2 或 3。选 2 命中 \u3000 前导的章节标题行。
    const out = splitChaptersByMarkers(mergedSegs, ["2"], raw);
    expect(out).toHaveLength(2);
    expect(out[0]?.title).toBe("第1章：我是好人");
    expect(out[1]?.title).toBe("第2章：这是个误会");
    expect(out[0]?.content).toBe("body1");
    expect(out[1]?.content).toBe("body2");
  });

  it("bodyStartByTitle tolerates invisible chars around stored title", () => {
    const raw = "第1章：我是好人\nbody1\n第2章：这是个误会\nbody2\n";
    const mergedSegs = [{
      title: "第1章：我是好人\u200B",
      content: "body1\n第2章：这是个误会\nbody2",
      word_count: 100,
    }];
    // marker 选 2,命中第2章标题行 (raw 第 2 行) —— 验证 bodyStart[0] 不是 -1。
    const diag = diagnoseSplit(mergedSegs, ["2"], raw);
    expect(diag.bodyStarts[0]).toBeGreaterThanOrEqual(0);
    expect(diag.negativeIndices).toEqual([]);
  });

  it("uses rawText title detection when merged content has no title line", () => {
    // 真实场景:mergeSuppressed 拼出来的 content 不包含标题行(只拼 body)。
    // marker 落在两章之间的标题行时,segLines[splitAt] 是下一章 body 的第一行,
    // 不是 chapter title —— 必须 fallback 到 rawText[mLine] 才能识别标题。
    const raw = "第1章：我是好人\nbody line 1 1 for rawText fallback test\nbody line 1 2 for rawText fallback test\n第2章：这是个误会\nbody line 2 1 for rawText fallback test\nbody line 2 2 for rawText fallback test\n";
    const rawLines = raw.split('\n');
    // rawLines:0=第1章, 1=body line 1 1 for rawText fallback test, 2=body line 1 2 for rawText fallback test, 3=第2章, 4=body line 2 1 for rawText fallback test, 5=body line 2 2 for rawText fallback test, 6=空
    // merged content:chapter1_body + chapter2_body(无标题行,模拟 mergeSuppressed)
    const mergedSegs = [{
      title: "第1章：我是好人",
      content: "body line 1 1 for rawText fallback test\nbody line 1 2 for rawText fallback test\nbody line 2 1 for rawText fallback test\nbody line 2 2 for rawText fallback test",
      word_count: 100,
    }];
    // marker "3" 命中 raw 第 3 行 = "第2章：这是个误会" —— 但 segLines[3-1=2] 是 "body line 2 1 for rawText fallback test"。
    const out = splitChaptersByMarkers(mergedSegs, ["3"], raw);
    expect(out).toHaveLength(2);
    expect(out[0]?.title).toBe("第1章：我是好人");
    expect(out[1]?.title).toBe("第2章：这是个误会");
  });

  it("keeps (续) suffix when marker lands on a long non-title body line", () => {
    // body 行超过 30 字 → 不算候选标题 → 下半段拿「(续)」后缀。
    const longBody = "这是一段明显超过三十字符的正文段落用来确认它不会作为标题候选 中文扩展";
    const raw = "T1\nbody1\n" + longBody + "\nbody3\n";
    const segs = [{ title: "T1", content: "body1\n" + longBody + "\nbody3", word_count: 100 }];
    // raw 行号：0=T1, 1=body1, 2=longBody, 3=body3 → marker 2 落在长 body 上
    const out = splitChaptersByMarkers(segs, ["2"], raw);
    expect(out).toHaveLength(2);
    expect(out[0]?.title).toBe("T1");
    expect(out[0]?.content).toBe("body1");
    expect(out[1]?.title).toBe("T1" + "\u201C\uFF08\u7EED\uFF09\u201D");
    expect(out[1]?.content).toBe(longBody + "\nbody3");
  });

  it("short non-title body line becomes the new chapter title", () => {
    // 用户报告：在「咚 咚 咚 ！」之类的短句上点「章」→ 期望成为新章节标题,
    // 而不是退化成「(续)」。
    const raw = "T1\nbody1\n咚 咚 咚 ！\nbody3\n";
    const segs = [{ title: "T1", content: "body1\n咚 咚 咚 ！\nbody3", word_count: 100 }];
    // raw 行号：0=T1, 1=body1, 2=咚 咚 咚 ！, 3=body3
    const out = splitChaptersByMarkers(segs, ["2"], raw);
    expect(out).toHaveLength(2);
    expect(out[0]?.title).toBe("T1");
    expect(out[0]?.content).toBe("body1");
    expect(out[1]?.title).toBe("咚 咚 咚 ！");
    expect(out[1]?.content).toBe("body3");
  });

  it("consecutive non-title markers stack (续) on the previous pushed title", () => {
    // 多段切分都落在长 body 行上(全是非标题),第二段应该是「上一段标题 + (续)」,
    // 而不是退化成「seg.title + (续)」(那样会让所有分段都长得一样)。
    const longBody = "这是一段明显超过三十字符的正文段落用来确认它不会作为标题候选 中文扩展";
    const raw = "T1\nbody line 1 for multi-split test\n" + longBody + "\n" + longBody + "\n" + longBody + "\n";
    const segs = [{ title: "T1", content: "body line 1 for multi-split test\n" + longBody + "\n" + longBody + "\n" + longBody, word_count: 100 }];
    // raw 行号：0=T1, 1=body line 1 for multi-split test, 2/3/4=三个长 body 行
    // markers 2, 3 → 在第 2 / 3 个长 body 行处切,共三段。
    const out = splitChaptersByMarkers(segs, ["2", "3"], raw);
    expect(out).toHaveLength(3);
    expect(out[0]?.title).toBe("T1");
    expect(out[0]?.content).toBe("body line 1 for multi-split test");
    // 第一处切 → 上一段标题是 seg.title → 第二段标题是「T1(续)」
    expect(out[1]?.title).toBe("T1" + "\u201C\uFF08\u7EED\uFF09\u201D");
    expect(out[1]?.content).toBe(longBody);
    // 第二处切 → 上一段标题已是「T1(续)」 → 第三段再叠一次。
    expect(out[2]?.title).toBe("T1" + "\u201C\uFF08\u7EED\uFF09\u201D" + "\u201C\uFF08\u7EED\uFF09\u201D");
    expect(out[2]?.content).toBe(longBody + "\n" + longBody);
  });



  it("does not push 0-word zombie chapter when marker hits last body line and that line is short title candidate", () => {
    // 场景:marker 落在 body 的最后一行,且该行 ≤30 字 → parseChapterTitle 视为下一段标题候选。
    // partStart 推到 segLines.length → lastClean = "" → countChapterChars = 0
    // → 不能 push 一行带标题却没正文的'僵尸'章节(防御 0字 bug)。
    //
    // 复现用户场景:章节解析页 chapter 2「第二章今世只想生孩子」字数显示 0,
    // 原因是切分时留了一个 title='xxx', content='' 的空章节。
    //
    // 注:marker line 被 consumed 作为 next chapter title,marker 后面没正文 →
    // next chapter 被跳过,marker line 内容也丢了(没地方放)。这是设计取舍:
    // 用户在最后一行加 marker 本身语义模糊,本测试只断言「不出现 0字章节」。
    const raw = "T1\nbody1\n咚 咚 咚 ！\n";
    // rawLines:0=T1, 1=body1, 2=咚 咚 咚 ！
    // seg.content = "body1\n咚 咚 咚 ！" → segLines = ["body1", "咚 咚 咚 ！"]
    // marker 2 → splitAt = 2 - 1 = 1
    //   part = ["body1"] → push "T1"+"body1" → OK
    //   detected = "咚 咚 咚 ！" (≤30 chars 标题候选)
    //   nextPartTitle = "咚 咚 咚 ！", partStart = 2
    // 离开 for, lastClean = "" → 0字 → SKIP push(zombie 防御)
    const segs = [{ title: "T1", content: "body1\n咚 咚 咚 ！", word_count: 7 }];
    const out = splitChaptersByMarkers(segs, ["2"], raw);
    // 没有 zombie 章节 → 只有原本的 T1 上半段。
    expect(out).toHaveLength(1);
    expect(out[0]?.title).toBe("T1");
    expect(out[0]?.content).toBe("body1");
    expect(out[0]?.word_count).toBe(5);
    // 任何残留都不应有 0 字章节。
    for (const s of out) {
      expect(s.word_count).toBeGreaterThan(0);
    }
  });

  it("does not push 0-word zombie chapter when marker splits off whitespace-only tail", () => {
    // 场景:marker 切完后半段只剩 whitespace → lastClean = "" → SKIP push。
    // 防御:marker 紧贴 chapter 末尾时,不能 push 一个空章节占位。
    const raw = "T1\nbody1\nbody2\n";
    // rawLines:0=T1, 1=body1, 2=body2
    // seg.content = "body1\nbody2" → segLines = ["body1", "body2"]
    // marker 2 → splitAt = 1
    //   part = ["body1"] → push "T1"+"body1" → OK
    //   detected = parseChapterTitle("body2") = "body2" (≤30 chars)
    //   nextPartTitle = "body2", partStart = 2
    // 离开 for, lastClean = "" → SKIP push
    const segs = [{ title: "T1", content: "body1\nbody2", word_count: 10 }];
    const out = splitChaptersByMarkers(segs, ["2"], raw);
    expect(out).toHaveLength(1);
    expect(out[0]?.title).toBe("T1");
    expect(out[0]?.content).toBe("body1");
    expect(out[0]?.word_count).toBe(5);
    for (const s of out) {
      expect(s.word_count).toBeGreaterThan(0);
    }
  });

  it("still pushes trailing chapter when lastClean has content (regression check on the guard)", () => {
    // 反向测试:guard 不应过度防御 —— 真正有内容的下半段仍然要 push。
    const raw = "T1\nbody1\nbody2\n";
    const segs = [{ title: "T1", content: "body1\nbody2\nbody3", word_count: 15 }];
    // segLines = ["body1", "body2", "body3"]
    // marker 2 → splitAt = 1
    //   part = ["body1"] → push "T1"+"body1" → OK
    //   detected = "body2" (≤30 chars)
    //   nextPartTitle = "body2", partStart = 2
    // 离开 for, lastRaw = segLines.slice(2) = ["body3"] → joined = "body3" → lastClean = "body3"
    //   countChapterChars("body3") = 5 > 0 → push "body2"+"body3"
    const out = splitChaptersByMarkers(segs, ["2"], raw);
    expect(out).toHaveLength(2);
    expect(out[0]?.title).toBe("T1");
    expect(out[0]?.content).toBe("body1");
    expect(out[1]?.title).toBe("body2");
    expect(out[1]?.content).toBe("body3");
    expect(out[1]?.word_count).toBe(5);
  });
});



describe('mergeSuppressed + splitChaptersByMarkers round trip', () => {
  /// 用户场景:chapter1 + chapter2(2047 字),先把 chapter2 suppress 合并到 chapter1,
  /// 再在 chapter2 的标题行(原 0-based 91 行)加 marker。
  /// 期望:split 后 workingChapters 恢复成 2 章,chapter2 的 title = "第二章今世只想生孩子"。
  it('restores chapter 2 when marker is added on its former title line after merge', () => {
    const text = 'content intro\n\n第一章：开篇\nbody1 line 1\nbody1 line 2\n第二章今世只想生孩子\nbody2 line 1\nbody2 line 2\nbody2 line 3\n';
    // 原始 segs
    const segs = [
      { title: '第一章：开篇', content: 'body1 line 1\nbody1 line 2', word_count: 6 },
      { title: '第二章今世只想生孩子', content: 'body2 line 1\nbody2 line 2\nbody2 line 3', word_count: 6 },
    ];
    // 模拟 mergeSuppressed:把 chapter2 的 content 拼到 chapter1 末尾,丢掉 chapter2。
    const merged = [{
      title: '第一章：开篇',
      content: segs[0].content + '\n' + segs[1].content,
      word_count: 12,
    }];
    // raw 行号:0=content intro, 1=空, 2=第一章, 3=body1 line 1, 4=body1 line 2,
    // 5=第二章今世只想生孩子, 6=body2 line 1, 7=body2 line 2, 8=body2 line 3
    // 在 chapter2 标题行(0-based 5)加 marker,期望 split 出 chapter2。
    const out = splitChaptersByMarkers(merged, ['5'], text);
    expect(out).toHaveLength(2);
    expect(out[0].title).toBe('第一章：开篇');
    expect(out[1].title).toBe('第二章今世只想生孩子');
    expect(out[0].content).toBe('body1 line 1\nbody1 line 2');
    expect(out[1].content).toBe('body2 line 1\nbody2 line 2\nbody2 line 3');
  });

  /// 用户报告的另一个变体:在 chapter2 body 第一行("咚咚咚!")加 marker。
  /// 期望:上半段继承 chapter1 标题,下半段拿到新标题(短句行被视为标题候选)。
  it('uses short body line as new chapter title when marker lands on it', () => {
    const text = '第一章：开篇\nbody1 line 1\nbody1 line 2\n第二章今世只想生孩子\n咚咚咚!\nbody2 line 1\nbody2 line 2\n';
    // 原始 segs
    const segs = [
      { title: '第一章：开篇', content: 'body1 line 1\nbody1 line 2', word_count: 6 },
      { title: '第二章今世只想生孩子', content: '咚咚咚!\nbody2 line 1\nbody2 line 2', word_count: 6 },
    ];
    const merged = [{
      title: '第一章：开篇',
      content: segs[0].content + '\n' + segs[1].content,
      word_count: 12,
    }];
    // raw 行号:0=第一章, 1=body1 line 1, 2=body1 line 2, 3=第二章, 4=咚咚咚!, 5=body2 line 1, 6=body2 line 2
    // 在 0-based 4 行(咚 咚 咚!)加 marker —— 标题候选行。
    const out = splitChaptersByMarkers(merged, ['4'], text);
    expect(out.length).toBeGreaterThanOrEqual(2);
    // 第二段标题应该是 "咚咚咚!"(parseChapterTitle 把 <=30 字符的短行视为标题)。
    expect(out[1].title).toBe('咚咚咚!');
  });

  /// 多个 marker 在同一 chapter 里:分别产生多段,标题累加 "(续)" 后缀。
  it('multiple markers in merged chapter produce multiple parts with (续) suffix', () => {
    // 这里用明顯超过三十字符的 body 作为分割点(parseChapterTitle 不会把它当标题)
    // —— 期望连续切两处后,下半段标题累加「(续)」后缀。
    const long = '这是一段明显超过三十字符的正文段落用来确认它不会作为标题候选';
    const text = '第一章：开篇\n1. body line 一\n' + long + '\n' + long + '\n' + long + '\n' + long + '\n';
    const merged = [{
      title: '第一章：开篇',
      content: '1. body line 一\n' + long + '\n' + long + '\n' + long + '\n' + long,
      word_count: 5,
    }];
    // raw 行号:0=第一章, 1='1. body line 一', 2/3/4/5 四个长 body 行
    // markers 2 和 3 在相邻两个长 body 行切,期望 3 段;后两段标题累加 (续)。
    const out = splitChaptersByMarkers(merged, ['2', '3'], text);
    expect(out).toHaveLength(3);
    expect(out[0].title).toBe('第一章：开篇');
    expect(out[0].content).toBe('1. body line 一');
    expect(out[1].title).toBe('第一章：开篇\u201C\uFF08\u7EED\uFF09\u201D');
    expect(out[1].content).toBe(long);
    expect(out[2].title).toBe('第一章：开篇\u201C\uFF08\u7EED\uFF09\u201D\u201C\uFF08\u7EED\uFF09\u201D');
    expect(out[2].content).toBe(long + '\n' + long + '\n' + long);
  });
});
