import { computed, ref, toValue, watch, type MaybeRefOrGetter } from 'vue';

export interface SearchableLine {
  line: number;
  text: string;
}

/// 章节内搜索组合式。
///
/// 参数接受 `MaybeRefOrGetter`:调用方可以传 ref(从 storeToRefs/toRef 拿到的真 ref)
/// 也可以传 getter(`() => store.rawLines` 这种)。Pinia setup store 字段会自动 unwrap,
/// 直接传 store 字段会拿到裸值,失去响应性 —— 用 getter 形式绕开这个问题。
export function useChapterSearch(
  query: MaybeRefOrGetter<string>,
  lines: MaybeRefOrGetter<readonly SearchableLine[]>,
) {
  const currentHitCursor = ref(0);

  const hitLineIndices = computed<number[]>(() => {
    const q = toValue(query);
    if (!q) return [];
    const out: number[] = [];
    const arr = toValue(lines);
    for (let i = 0; i < arr.length; i++) {
      if (arr[i].text.includes(q)) out.push(i);
    }
    return out;
  });

  // 查询或行集合变化 → 把游标重置到第一个命中。
  // watch 源用 getter 形式:即便调用方传入裸数组,Vue 也能正确把它视作 watchable getter;
  // 传入 ref/getter 时,Vue 会自动追踪内部响应性(无需传 Ref 类型强约束)。
  watch(
    [() => toValue(query), () => toValue(lines)],
    () => {
      currentHitCursor.value = 0;
    },
    { flush: 'sync' },
  );

  const hitCount = computed(() => hitLineIndices.value.length);
  const canPrev = computed(() => hitCount.value > 0);
  const canNext = computed(() => hitCount.value > 0);

  const currentHitLineIndex = computed<number>(() => {
    if (hitCount.value === 0) return -1;
    const safe = Math.min(currentHitCursor.value, hitCount.value - 1);
    return hitLineIndices.value[safe] ?? -1;
  });

  function next() {
    if (hitCount.value === 0) return;
    const safe = Math.min(currentHitCursor.value, hitCount.value - 1);
    currentHitCursor.value = (safe + 1) % hitCount.value;
  }

  function prev() {
    if (hitCount.value === 0) return;
    const safe = Math.min(currentHitCursor.value, hitCount.value - 1);
    currentHitCursor.value = (safe - 1 + hitCount.value) % hitCount.value;
  }

  return {
    hitLineIndices,
    hitCount,
    currentHitLineIndex,
    canPrev,
    canNext,
    next,
    prev,
  };
}
