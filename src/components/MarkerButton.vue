<template>
  <button class="seal" type="button" :title="title" @click="emit('mark')">
    章
  </button>
</template>

<script setup lang="ts">
defineProps<{ title?: string }>();
const emit = defineEmits<{ mark: [] }>();
</script>

<style scoped>
/*
  Signature: 红印章 "章" — 把 marker action 视觉上绑定到"在原文盖印"
  方块底 + 红色字 + 1px 朱砂边;hover 时下沉成深朱
*/
.seal {
  width: 22px;
  height: 22px;
  padding: 0;
  background: var(--color-sheet);
  border: 1px solid var(--color-cinnabar);
  color: var(--color-cinnabar);
  font-family: var(--font-serif);
  font-size: 14px;
  font-weight: var(--font-weight-medium);
  line-height: 20px;
  cursor: pointer;
  border-radius: 2px;
  flex-shrink: 0;
  letter-spacing: 0;
  transition: background 0.1s, color 0.1s;
}
.seal:hover {
  background: var(--color-cinnabar);
  color: #faf6ee;
}
</style>
