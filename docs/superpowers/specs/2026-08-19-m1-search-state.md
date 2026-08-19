# M1 — 把搜索状态从 store 抽离到 composable

日期：2026-08-19
范围：\`src/stores/chapters.ts\` + \`src/views/parse.vue\` + \`src/composables/useChapterSearch.ts\`

## 现状

\`chapters.ts\` 同时持有一组\"搜索状态\"：

\`\`\`\`\`\`\`\`\`ts
const searchQuery = ref<string>('');
const currentHitIndex = ref<number>(0);
function setSearchQuery(q: string) { ... currentHitIndex.value = 0; }
function nextSearchHit(total: number) { ... }
function prevSearchHit(total: number) { ... }
\`\`\`\`\`\`\`\`\`ts

\`useChapterSearch.ts\` 是一组独立 composable，自己管 \`currentHitCursor\`：

\`\`\`\`\`\`\`\`\`ts
useChapterSearch(query: MaybeRefOrGetter<string>, lines: MaybeRefOrGetter<readonly SearchableLine[]>)
→ { hitLineIndices, hitCount, currentHitLineIndex, canPrev, canNext, next, prev }
\`\`\`\`\`\`\`\`\`

## 实际调用关系

- parse.vue 用 \`store.searchQuery\` / \`store.setSearchQuery\` 作 query 输入框双向绑定
- parse.vue 把 \`computed(get/set)\` 包出的 \`searchQueryRef\` 传给 \`useChapterSearch\`
- parse.vue 用 composable 的 \`next / prev\` —— **不用** store 的 \`nextSearchHit / prevSearchHit\`
- parse.vue 用 composable 的 \`currentHitLineIndex\` 做滚动定位
- \`store.currentHitIndex\` / \`store.nextSearchHit\` / \`store.prevSearchHit\` **没有任何调用方**

store 那套搜索状态是死代码（除了 query 本身）。

## 目标

1. 删 store 里 \`currentHitIndex\` / \`nextSearchHit\` / \`prevSearchHit\` 死代码
2. \`searchQuery\` 从 store 抽到 parse.vue 局部状态（不属于跨页面共享的业务数据）
3. composable 接口**不变** —— 它本来已经接受 \`MaybeRefOrGetter<string>\`，传 \`Ref<string>\` 即可
4. 搜索 UI 状态彻底自洽在 useChapterSearch 内部，未来其他页面需要时直接复用

## 改动

### \`src/stores/chapters.ts\`

删除：

- \`const searchQuery = ref<string>('');\`
- \`const currentHitIndex = ref<number>(0);\`
- \`function setSearchQuery(q: string)\`
- \`function nextSearchHit(total: number)\`
- \`function prevSearchHit(total: number)\`
- \`load()\` 内的 \`searchQuery.value = ''; currentHitIndex.value = 0;\`
- \`unload()\` 内的 \`searchQuery.value = ''; currentHitIndex.value = 0;\`
- return 里的 \`searchQuery, currentHitIndex, setSearchQuery, nextSearchHit, prevSearchHit\`

不改：
- \`rawLines\`（composable 的 lines 源，仍来自 store）
- 任何业务数据（chapters / markers / suppressed / titleOverrides / ...）

### \`src/views/parse.vue\`

替换：

\`\`\`\`\`\`\`\`\`ts
// before
const searchQueryRef = computed({
  get: () => store.searchQuery,
  set: (v: string) => store.setSearchQuery(v),
});
const search = useChapterSearch(searchQueryRef, () => store.rawLines);

// after
const searchQuery = ref<string>('');
const search = useChapterSearch(searchQuery, () => store.rawLines);
\`\`\`\`\`\`\`\`\`

调整模板：

- 输入框 \`:value=\"store.searchQuery\"\` + \`@input=\"onSearchInput\"\` → \`v-model=\"searchQuery\"\`
- \`onSearchInput(value: string)\` 删函数，模板上直接 \`@input=\"(e) => { searchQuery.value = (e.target as HTMLInputElement).value; scrollToActiveHit(); }\"`

或更干净的方案：保留 \`onSearchInput\` 函数，内部改成直接赋值 \`searchQuery.value = value\`，不再走 store。

### \`src/composables/useChapterSearch.ts\`

不改。**接口已经满足要求**。

## 风险评估

| 风险点 | 评估 |
|---|---|
| 接口变化 | 0 —— composable 不变，parse.vue 改成传 \`Ref<string>\`（已支持） |
| 响应性丢失 | 0 —— \`MaybeRefOrGetter<string>\` 路径已覆盖，传 ref 时 Vue 自动追踪 |
| 跨页面共享 | 不需要 —— 搜索状态本来只在 parse 页用 |
| 持久化 | 不做 —— 离开页面 = 状态清空（现状行为） |
| 调用方遗漏 | \`rg \"store\\.(searchQuery|setSearchQuery|nextSearchHit|prevSearchHit|currentHitIndex)\" src\` 应该是空 |
| 类型检查 | \`vue-tsc --noEmit\` 应无新错误 |

## 拒绝的范围

- ❌ composable 改大小写不敏感（现状是敏感，无需求）
- ❌ searchQuery 持久化到 URL / localStorage（无需求）
- ❌ 现在就改 Library 等其他页面使用 composable（当前无需求，但保留可能性）
- ❌ 把 composable 下沉到 store（违反分层边界）

## 验收测试

1. \`npm run tauri dev\` 启动
2. 进\"上传 → 解析章节\"
3. 输入搜索词 → 高亮命中行，计数器显示 \`N / Total\`
4. 上下键 / 点击翻页按钮 → 计数器变化，文本滚动跟随
5. 清空搜索 → 计数器显示 \`0 / 0\`
6. 切换到其他章节再切回 → 搜索词清空（行为不变）
7. 离开页面再回来 → 搜索状态空白（局部 ref 行为）

\`vue-tsc --noEmit\`：仅剩 5 个已知历史欠账，无新增错误。
\`rg \"store\\.(searchQuery|setSearchQuery|nextSearchHit|prevSearchHit|currentHitIndex)\" src\`：空。
\`git diff --stat\`：仅 \`stores/chapters.ts\` + \`views/parse.vue\` 改动。
