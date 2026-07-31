import { computed, ref, watch, type Ref } from 'vue';

export interface SearchableLine {
  byte_start: number;
  text: string;
}

export function useChapterSearch(
  query: Ref<string>,
  lines: Ref<readonly SearchableLine[]>,
) {
  const currentHitCursor = ref(0);

  const hitLineIndices = computed<number[]>(() => {
    const q = query.value;
    if (!q) return [];
    const out: number[] = [];
    const arr = lines.value;
    for (let i = 0; i < arr.length; i++) {
      if (arr[i].text.includes(q)) out.push(i);
    }
    return out;
  });

  // 查询或行集合变化 → 把游标重置到第一个命中。
  watch(
    [query, lines],
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
