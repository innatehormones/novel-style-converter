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
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <template v-if="item.icon === 'upload'">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
            <polyline points="17 8 12 3 7 8" />
            <line x1="12" y1="3" x2="12" y2="15" />
          </template>
          <template v-else-if="item.icon === 'convert'">
            <polyline points="17 1 21 5 17 9" />
            <path d="M3 11V9a4 4 0 0 1 4-4h14" />
            <polyline points="7 23 3 19 7 15" />
            <path d="M21 13v2a4 4 0 0 1-4 4H3" />
          </template>
          <template v-else-if="item.icon === 'data'">
            <ellipse cx="12" cy="5" rx="9" ry="3" />
            <path d="M3 5v6c0 1.66 4 3 9 3s9-1.34 9-3V5" />
            <path d="M3 11v6c0 1.66 4 3 9 3s9-1.34 9-3v-6" />
          </template>
          <template v-else>
            <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" />
            <polyline points="3.27 6.96 12 12.01 20.73 6.96" />
            <line x1="12" y1="22.08" x2="12" y2="12" />
          </template>
        </svg>
        <span class="label">{{ item.label }}</span>
      </router-link>
    </nav>

    <nav class="nav nav-bottom">
      <button
        class="nav-item theme-btn"
        :title="theme.theme === 'light' ? '切换到暗色' : '切换到亮色'"
        @click="theme.toggle()"
      >
        <svg v-if="theme.theme === 'light'" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="5" />
          <line x1="12" y1="1" x2="12" y2="3" />
          <line x1="12" y1="21" x2="12" y2="23" />
          <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
          <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
          <line x1="1" y1="12" x2="3" y2="12" />
          <line x1="21" y1="12" x2="23" y2="12" />
          <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
          <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
        </svg>
        <svg v-else width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
        </svg>
        <span class="label">{{ theme.theme === 'light' ? '暗色' : '亮色' }}</span>
      </button>
    </nav>
  </aside>
</template>

<script setup lang="ts">
import { useRoute } from 'vue-router';
import { useThemeStore } from '../stores/theme';

interface Item { to: string; label: string; icon: 'upload' | 'data' | 'convert' | 'model' }

const topItems: Item[] = [
  { to: '/uploads', label: '上传', icon: 'upload' },
  { to: '/data-assets', label: '数据资产', icon: 'data' },
  { to: '/transformations', label: '转换', icon: 'convert' },
  { to: '/models', label: '模型', icon: 'model' },
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
