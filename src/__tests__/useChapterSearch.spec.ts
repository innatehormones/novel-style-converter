// @vitest-environment happy-dom
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ref, nextTick, type Ref } from 'vue';
import { useChapterSearch } from '../composables/useChapterSearch';

interface Line { byte_start: number; text: string }

const lines: Line[] = [
  { byte_start: 0, text: '寒山远黛，秋水共长天一色' },
  { byte_start: 10, text: '落霞与孤鹜齐飞' },
  { byte_start: 20, text: '秋水再次出现于江畔' },
  { byte_start: 30, text: '与孤鹜齐飞共入画卷' },
  { byte_start: 40, text: '寻常巷陌，灯火可亲' },
];

describe('useChapterSearch', () => {
  let query: Ref<string>;
  let linesRef: Ref<Line[]>;

  beforeEach(() => {
    query = ref('');
    linesRef = ref(lines);
  });

  it('空查询下没有命中', () => {
    const s = useChapterSearch(query, linesRef);
    expect(s.hitLineIndices.value).toEqual([]);
    expect(s.hitCount.value).toBe(0);
    expect(s.currentHitLineIndex.value).toBe(-1);
    expect(s.canPrev.value).toBe(false);
    expect(s.canNext.value).toBe(false);
  });

  it('收集所有命中行', async () => {
    query.value = '秋水';
    await nextTick();
    const s = useChapterSearch(query, linesRef);
    expect(s.hitLineIndices.value).toEqual([0, 2]);
    expect(s.hitCount.value).toBe(2);
  });

  it('初始当前命中 = 0，翻页顺序循环', async () => {
    query.value = '秋水';
    await nextTick();
    const s = useChapterSearch(query, linesRef);
    expect(s.currentHitLineIndex.value).toBe(0);

    s.next();
    expect(s.currentHitLineIndex.value).toBe(2);

    s.next();
    // 循环回第一个命中
    expect(s.currentHitLineIndex.value).toBe(0);

    s.prev();
    // 反向循环
    expect(s.currentHitLineIndex.value).toBe(2);
  });

  it('查询变化时把当前命中重置为 0', async () => {
    query.value = '秋水';
    await nextTick();
    const s = useChapterSearch(query, linesRef);
    s.next();
    s.next();
    expect(s.currentHitLineIndex.value).toBe(0);

    query.value = '孤鹜';
    await nextTick();
    expect(s.hitLineIndices.value).toEqual([1, 3]);
    expect(s.currentHitLineIndex.value).toBe(1);
  });

  it('命中为空时翻页不应改变 currentHitLineIndex', async () => {
    query.value = 'xyz不存在的字符串';
    await nextTick();
    const s = useChapterSearch(query, linesRef);
    s.next();
    s.prev();
    expect(s.currentHitLineIndex.value).toBe(-1);
  });

  it('lines 引用变化后命中行会基于最新 lines 重新计算', async () => {
    query.value = '秋水';
    await nextTick();
    const s = useChapterSearch(query, linesRef);
    expect(s.hitLineIndices.value).toEqual([0, 2]);

    linesRef.value = [{ byte_start: 0, text: '秋水又来一遍' }];
    await nextTick();
    expect(s.hitLineIndices.value).toEqual([0]);
  });

  it('canPrev/canNext 在空查询时为 false', () => {
    const s = useChapterSearch(query, linesRef);
    expect(s.canPrev.value).toBe(false);
    expect(s.canNext.value).toBe(false);
  });
});
