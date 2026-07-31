<template>
  <div class="wrap">
    <table class="table">
      <thead>
        <tr>
          <th
            v-for="col in columns"
            :key="col.key"
            :style="{ width: typeof col.width === 'number' ? `${col.width}px` : col.width ?? 'auto' }"
          >
            {{ col.title }}
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-if="data.length === 0">
          <td :colspan="columns.length" class="empty">{{ emptyText }}</td>
        </tr>
        <tr v-for="(row, i) in data" :key="rowKey(row, i)">
          <td v-for="col in columns" :key="col.key">
            <slot :name="`cell-${col.key}`" :row="row">
              {{ row[col.key] }}
            </slot>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    columns: { key: string; title: string; width?: string | number }[];
    data: any[];
    emptyText?: string;
    rowKey?: (row: any, idx: number) => string | number;
  }>(),
  {
    emptyText: '暂无数据',
    rowKey: (_row: any, idx: number) => idx,
  },
);
</script>

<style scoped>
/*
  ruled-manuscript table: 白卡浮在米纸, 行间用浅 hairline,
  th 用衬线 small caps 风格, hover 红线突出
*/
.wrap {
  width: 100%;
  background: var(--color-sheet);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-card);
  overflow: hidden;
  box-shadow: var(--shadow);
}
.table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13.5px;
}
.table th,
.table td {
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-soft);
  text-align: left;
}
.table th {
  background: transparent;
  font-family: var(--font-serif);
  font-weight: var(--font-weight-regular);
  font-size: 12px;
  color: var(--text-muted);
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
.table td {
  color: var(--text-primary);
}
.table tbody tr:last-child td {
  border-bottom: none;
}
.table tbody tr:hover td {
  background: var(--color-paper);
}
.empty {
  text-align: center;
  color: var(--text-muted);
  padding: 48px 0;
  font-family: var(--font-serif);
  font-style: italic;
}
</style>
