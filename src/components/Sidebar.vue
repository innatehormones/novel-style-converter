<template>
  <aside class="sidebar">
    <nav class="nav">
      <router-link
        v-for="item in topItems"
        :key="item.to"
        :to="item.to"
        class="nav-item"
        :class="{ active: isActive(item.to) }"
      >
        <component :is="item.icon" :size="16" :stroke-width="1.5" />
        <span class="label">{{ item.label }}</span>
      </router-link>
    </nav>

    <nav class="nav nav-bottom">
      <button
        class="nav-item theme-btn"
        :title="theme.theme === 'light' ? '切换到暗色' : '切换到亮色'"
        @click="theme.toggle()"
      >
        <component :is="theme.theme === 'light' ? IconSun : IconMoon" :size="16" :stroke-width="1.5" />
        <span class="label">{{ theme.theme === 'light' ? '暗色' : '亮色' }}</span>
      </button>
    </nav>
  </aside>
</template>

<script setup lang="ts">
import { markRaw, type Component } from 'vue';
import { useRoute } from 'vue-router';
import IconUpload from '~icons/lucide/upload';
import IconDatabase from '~icons/lucide/database';
import IconRepeat from '~icons/lucide/repeat';
import IconFileText from '~icons/lucide/file-text';
import IconBox from '~icons/lucide/box';
import IconActivity from '~icons/lucide/activity';
import IconSun from '~icons/lucide/sun';
import IconMoon from '~icons/lucide/moon';
import { useThemeStore } from '../stores/theme';

interface Item { to: string; label: string; icon: Component }

const topItems: Item[] = [
  { to: '/uploads', label: '上传', icon: markRaw(IconUpload) },
  { to: '/data-assets', label: '数据资产', icon: markRaw(IconDatabase) },
  { to: '/transformations', label: '转换工作区', icon: markRaw(IconRepeat) },
  { to: '/prompts', label: '提示词', icon: markRaw(IconFileText) },
  { to: '/models', label: '模型', icon: markRaw(IconBox) },
  { to: '/ai-calls', label: 'AI 调用', icon: markRaw(IconActivity) },
];

const route = useRoute();
const theme = useThemeStore();

function isActive(to: string): boolean {
  if (route.path === to) return true;
  if (to === '/uploads') return route.path.startsWith('/library/upload/');
  if (to === '/data-assets') return route.path.startsWith('/library/data/');
  if (to === '/transformations') return route.path.startsWith('/library/transformation');
  return false;
}
</script>

<style scoped>
.sidebar {
  width: 220px;
  flex-shrink: 0;
  background: var(--bg-sidebar);
  border-right: 1px solid var(--border-soft);
  display: flex;
  flex-direction: column;
  padding: 24px 0 16px;
  box-sizing: border-box;
  height: 100vh;
}
.sidebar::before {
  /* 左侧 1 道细红印条 — 跟内容区的红线呼应 */
  content: '';
  position: absolute;
  left: 219px;
  top: 0;
  bottom: 0;
  width: 1px;
  background: var(--border-rouge);
  pointer-events: none;
}
.nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 0 12px;
}
.nav-bottom {
  margin-top: auto;
  padding-top: 16px;
  border-top: 1px solid var(--border-soft);
}
.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border-radius: var(--radius-pin);
  color: var(--text-secondary);
  text-decoration: none;
  background: transparent;
  border: none;
  cursor: pointer;
  font-size: 14px;
  font-family: inherit;
  width: 100%;
  box-sizing: border-box;
  text-align: left;
}
.nav-item:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}
.nav-item.active {
  /* 朱砂印章 active: 浅朱底 + 朱字 + 左侧一道窄红条 */
  background: var(--color-cinnabar-light);
  color: var(--color-cinnabar-deep);
  font-weight: var(--font-weight-medium);
  position: relative;
}
.nav-item.active::before {
  content: '';
  position: absolute;
  left: 0;
  top: 6px;
  bottom: 6px;
  width: 2px;
  background: var(--color-cinnabar);
}
.theme-btn { font-size: 13px; color: var(--text-muted); }
.label { white-space: nowrap; }
</style>
