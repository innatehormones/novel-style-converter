# Parse Right Pane CodeMirror 6 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the right pane of `src/views/parse.vue` (a `vue-virtual-scroller` `RecycleScroller` with fixed `:item-size="24"` that ellipsizes long lines) with a CodeMirror 6 read-only editor, so that long paragraphs wrap at viewport width and 10+ MB Chinese novels render without UI freeze.

**Architecture:** One new lazy-import composable `src/composables/useParseEditor.ts` owns the CM6 lifecycle (mount, theme, marker gutter, search integration, jump, destroy). `src/views/parse.vue` consumes it through a single `<div ref="cmHost" />` host. Left chapter list (`DynamicScroller` + title `<input>` + "并入上一章") and the `chapters` Pinia store are untouched. The composable mirrors established patterns from `src/views/Upload.vue` (committed `054a34e perf(editor): 上传原文 textarea -> CodeMirror 6 虚拟化渲染`) and `src/components/CleaningDialog.vue` (committed `ce7d27d perf(editor): 清洗预览 textarea -> 只读 CodeMirror 6`).

**Verification surface:** The parse page currently has zero test coverage (placeholder test files). Per spec §7 this plan does NOT add unit tests. Instead, each task verifies with `pnpm test` (existing vitest suite still green) and `pnpm build` (TypeScript + Vite build green). The final task adds a manual smoke checklist run against `pnpm dev` to confirm functional acceptance.

**Tech Stack:** Vue 3.5, Pinia 2, vue-virtual-scroller 2 (still used by `DataAsset.vue`), `@codemirror/state ^6.7`, `@codemirror/view ^6.43`, `@codemirror/commands ^6.11`, `@codemirror/search ^6.5` (newly added), TypeScript 5.6, Vite 6, vitest 2, happy-dom 15.

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `src/composables/useParseEditor.ts` | NEW | CM6 lifecycle composable: lazy-import state/view/commands/search, build read-only EditorView, marker gutter, search integration, scrollToLine, destroy. |
| `src/views/parse.vue` | MODIFY | Replace right-pane `RecycleScroller` with `<div ref="cmHost" />`; consume composable. CSS gets `.cm-marker-line` / `.cm-gutter.cm-marker-stamp` rules. Other parse.vue structure unchanged. |
| `src/composables/useChapterSearch.ts` | DELETE | Sole consumer was parse.vue; replacement lives inside the new composable. |
| `src/__tests__/useChapterSearch.spec.ts` | DELETE | Body is `export {};` placeholder. |
| `src/components/MarkerButton.vue` | DELETE | Sole consumer was parse.vue; stamp rendering moves into composable's gutter `lineMarker`. |
| `src/utils/format.ts` | MODIFY | Remove `useChapterSearch` comment cross-references only. No behavioral change. |
| `src/utils/status-locale.ts` | MODIFY | Remove `useChapterSearch` comment cross-references only. No behavioral change. |
| `package.json` / `pnpm-lock.yaml` | MODIFY | Add `@codemirror/search` dependency. |

`vue-virtual-scroller` package and `src/types/vue-virtual-scroller.d.ts` stay — `DataAsset.vue` still needs them.

---

## Tasks

### Task 1: Add `@codemirror/search` dependency

**Files:**
- Modify: `package.json`

- [ ] **Step 1: Add the dependency**

Open `package.json` and inside the `"dependencies"` block add a new line:

```json
"@codemirror/search": "^6.5.8",
```

Place it alphabetically (immediately after `@codemirror/view`, before `@floating-ui/dom`).

- [ ] **Step 2: Install**

Run:

```bash
pnpm install
```

Expected: lockfile updates; `node_modules/@codemirror/search` populated.

- [ ] **Step 3: Verify import resolves**

Run:

```bash
node -e "import('@codemirror/search').then(m => console.log(Object.keys(m).slice(0,8)))"
```

Expected output (order may vary): `[ 'search', 'findNext', 'findPrevious', 'getSearchQuery', 'setSearchQuery', 'searchKeymap', 'openSearchPanel', 'SearchQuery' ]` (8+ keys).

- [ ] **Step 4: Commit**

```bash
git add package.json pnpm-lock.yaml
git commit -m "chore(deps): add @codemirror/search for parse page text search"
```

---

### Task 2: Create `useParseEditor` skeleton (mount/destroy only)

**Files:**
- Create: `src/composables/useParseEditor.ts`

- [ ] **Step 1: Write the skeleton**

Create `src/composables/useParseEditor.ts` with the following content:

```ts
import { onBeforeUnmount, shallowRef, type Ref } from 'vue';
import type { EditorView as EditorViewType } from '@codemirror/view';

/// Composable that owns a CodeMirror 6 EditorView lifecycle.
/// Pattern mirrors Upload.vue / CleaningDialog.vue: dynamic-import all CM6
/// chunks, build a read-only view attached to a host <div>, expose the
/// public surface other tasks extend, destroy on unmount.
export interface UseParseEditorOptions {
  host: Ref<HTMLElement | null>;
  /// 0-based line numbers of marked lines.
  onMarkerToggle?: (line1based: number) => void;
}

export interface UseParseEditorApi {
  view: Readonly<Ref<EditorViewType | null>>;
  setMarkers: (lines1based: ReadonlySet<number>) => void;
  scrollToLine: (line0based: number) => void;
  runSearch: (query: string) => void;
  nextHit: () => void;
  prevHit: () => void;
  hitCount: Readonly<Ref<number>>;
  currentHitIndex: Readonly<Ref<number>>;
  replaceDoc: (text: string) => void;
  destroy: () => void;
}

export function useParseEditor(opts: UseParseEditorOptions): UseParseEditorApi {
  const view = shallowRef<EditorViewType | null>(null);
  const hitCount = shallowRef(0);
  const currentHitIndex = shallowRef(0);

  function destroy() {
    view.value?.destroy();
    view.value = null;
  }

  async function mount(_doc: string): Promise<void> {
    // Full implementation lands in Task 3.
  }

  onBeforeUnmount(destroy);

  return {
    view,
    setMarkers: (_) => { /* Task 4 */ },
    scrollToLine: (_) => { /* Task 7 */ },
    runSearch: (_) => { /* Task 6 */ },
    nextHit: () => { /* Task 6 */ },
    prevHit: () => { /* Task 6 */ },
    hitCount,
    currentHitIndex,
    replaceDoc: (_) => { /* Task 3 */ },
    destroy,
    mount,
  } as unknown as UseParseEditorApi;
}
```

- [ ] **Step 2: Sanity-check types**

Run:

```bash
pnpm exec vue-tsc --noEmit -p tsconfig.json
```

Expected: 0 errors. The placeholder bodies are explicit no-op stubs.

- [ ] **Step 3: Commit**

```bash
git add src/composables/useParseEditor.ts
git commit -m "feat(parse): scaffold useParseEditor composable with destroy lifecycle"
```

---


### Task 3: Implement read-only `EditorView` construction + theme

**Files:**
- Modify: `src/composables/useParseEditor.ts`

- [ ] **Step 1: Replace the composable body with full EditorView build**

Replace the entirety of `src/composables/useParseEditor.ts` with:

```ts
import { onBeforeUnmount, shallowRef, type Ref } from 'vue';
import type { EditorView as EditorViewType } from '@codemirror/view';

export interface UseParseEditorOptions {
  host: Ref<HTMLElement | null>;
  onMarkerToggle?: (line1based: number) => void;
}

export interface UseParseEditorApi {
  view: Readonly<Ref<EditorViewType | null>>;
  setMarkers: (lines1based: ReadonlySet<number>) => void;
  scrollToLine: (line0based: number) => void;
  runSearch: (query: string) => void;
  nextHit: () => void;
  prevHit: () => void;
  hitCount: Readonly<Ref<number>>;
  currentHitIndex: Readonly<Ref<number>>;
  replaceDoc: (text: string) => void;
  destroy: () => void;
}

export function useParseEditor(opts: UseParseEditorOptions): UseParseEditorApi {
  const view = shallowRef<EditorViewType | null>(null);
  const hitCount = shallowRef(0);
  const currentHitIndex = shallowRef(0);

  function destroy() {
    view.value?.destroy();
    view.value = null;
  }

  async function mount(doc: string): Promise<void> {
    const host = opts.host.value;
    if (!host) return;
    view.value?.destroy();
    view.value = null;

    const [
      { EditorState },
      { EditorView, drawSelection, lineNumbers },
      cmCommands,
      cmSearch,
    ] = await Promise.all([
      import('@codemirror/state'),
      import('@codemirror/view'),
      import('@codemirror/commands'),
      import('@codemirror/search'),
    ]);

    const themeExt = EditorView.theme({
      '&': {
        height: '100%',
        fontSize: '13px',
        fontFamily: 'var(--font-mono), ui-monospace, monospace',
        color: 'var(--text-primary)',
        backgroundColor: 'var(--color-sheet)',
      },
      '&.cm-focused': { outline: 'none' },
      '.cm-content': {
        padding: '8px 12px',
        caretColor: 'var(--color-cinnabar)',
      },
      '.cm-scroller': { fontFamily: 'inherit' },
      '.cm-gutters': {
        backgroundColor: 'transparent',
        borderRight: '1px solid var(--border-color)',
        color: 'var(--text-muted)',
      },
    }, { dark: false });

    view.value = new EditorView({
      state: EditorState.create({
        doc,
        extensions: [
          EditorState.readOnly.of(true),
          drawSelection(),
          EditorView.lineWrapping,
          lineNumbers(),
          themeExt,
          cmCommands.history(),
          cmSearch.search({ top: true }),
        ],
      }),
      parent: host,
    });
  }

  function replaceDoc(text: string): void {
    const v = view.value;
    if (!v) return;
    v.dispatch({
      changes: { from: 0, to: v.state.doc.length, insert: text },
    });
  }

  onBeforeUnmount(destroy);

  return {
    view,
    setMarkers: (_) => { /* Task 4 */ },
    scrollToLine: (_) => { /* Task 7 */ },
    runSearch: (_) => { /* Task 6 */ },
    nextHit: () => { /* Task 6 */ },
    prevHit: () => { /* Task 6 */ },
    hitCount,
    currentHitIndex,
    replaceDoc,
    destroy,
    mount,
  } as unknown as UseParseEditorApi;
}
```

- [ ] **Step 2: Type-check**

```bash
pnpm exec vue-tsc --noEmit -p tsconfig.json
```

Expected: 0 errors.

- [ ] **Step 3: Commit**

```bash
git add src/composables/useParseEditor.ts
git commit -m "feat(parse): construct read-only CM6 EditorView with theme"
```

---


### Task 4: Marker StateField + line-range decoration

**Files:**
- Modify: `src/composables/useParseEditor.ts`

- [ ] **Step 1: Extend the file to add the marker StateField and rebuild effect**

Add the following block of imports at the top (below the existing import lines):

```ts
import { StateField, StateEffect, RangeSetBuilder } from '@codemirror/state';
import { Decoration } from '@codemirror/view';
```

Then locate the `mount(doc)` function inside the composable and add the following block immediately before `mount` (so the StateField is defined at composable scope):

```ts
  // Marker StateField is built once per mount.
  const markerEffect = new StateEffect<ReadonlySet<number>>();
  const markerField = StateField.define<ReadonlySet<number>>({
    create: () => new Set<number>(),
    update: (value, tr) => {
      for (const e of tr.effects) {
        if (e.is(markerEffect)) return e.value;
      }
      return value;
    },
  });

  // Decoration builder: rebuild RangeSet of "marked line" decorations on marker change.
  const markerLineDeco = (view: EditorViewType) => {
    const set = view.state.field(markerField, false) ?? new Set<number>();
    const builder = new RangeSetBuilder<Decoration>();
    for (const line1based of set) {
      try {
        const line = view.state.doc.line(line1based);
        builder.add(line.from, line.from, Decoration.line({ attributes: { class: 'cm-marker-line' } }));
      } catch {
        // line out of range (e.g. doc shrunk); skip
      }
    }
    return builder.finish();
  };
  void markerLineDeco; // referenced in Task 5
```

- [ ] **Step 2: Wire `setMarkers` public API to dispatch the effect**

Replace the body of `setMarkers` in the returned object:

```ts
    setMarkers: (lines1based: ReadonlySet<number>) => {
      const v = view.value;
      if (!v) return;
      v.dispatch({ effects: markerEffect.of(new Set(lines1based)) });
    },
```

- [ ] **Step 3: Type-check**

```bash
pnpm exec vue-tsc --noEmit -p tsconfig.json
```

Expected: 0 errors.

- [ ] **Step 4: Commit**

```bash
git add src/composables/useParseEditor.ts
git commit -m "feat(parse): mirror marker lines via CM6 StateField"
```

---

### Task 5: Marker gutter `lineMarker` widget + click handler

**Files:**
- Modify: `src/composables/useParseEditor.ts`

- [ ] **Step 1: Add the marker-gutter extension declaration**

Add the `markerGutter` declaration inside the composable body (just before `mount`):

```ts
  let stampNode: HTMLElement | null = null;
  function ensureStamp(): HTMLElement {
    if (stampNode) return stampNode;
    const el = document.createElement('button');
    el.type = 'button';
    el.className = 'cm-marker-stamp';
    el.title = '取消标记';
    el.textContent = '章';
    stampNode = el;
    return el;
  }
  const markerGutter = {
    class: 'cm-marker-stamp',
    domEventHandlers: {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      click(_view: EditorViewType, lineView: any) {
        const v = view.value;
        if (!v || !opts.onMarkerToggle) return false;
        const line1based = v.state.doc.lineAt(lineView.from).number;
        opts.onMarkerToggle(line1based);
        return true;
      },
    },
    lineMarker(_view: EditorViewType, lineBlock: { from: number }) {
      const v = view.value;
      if (!v) return null;
      const set = v.state.field(markerField, false);
      if (!set) return null;
      const line1based = v.state.doc.lineAt(lineBlock.from).number;
      if (!set.has(line1based)) return null;
      return ensureStamp();
    },
  } as const;
```

- [ ] **Step 2: Add `gutter` to the destructure list and register gutter + decorations**

Locate the destructured imports inside `mount()`:

```ts
    const [
      { EditorState },
      { EditorView, drawSelection, lineNumbers },
      cmCommands,
      cmSearch,
    ] = await Promise.all([ ... ]);
```

Add `gutter` to the second tuple element so the destructured shape remains identical and T3's existing `EditorView / drawSelection / lineNumbers` references stay valid:

```ts
    const [
      { EditorState },
      { EditorView, drawSelection, lineNumbers, gutter },
      cmCommands,
      cmSearch,
    ] = await Promise.all([ ... ]);
```

Then in the `extensions: [...]` array, add the decorations extension and the gutter extension. Place these AFTER `markerField` and BEFORE `cmSearch.search`:

```ts
        extensions: [
          EditorState.readOnly.of(true),
          drawSelection(),
          EditorView.lineWrapping,
          lineNumbers(),
          themeExt,
          cmCommands.history(),
          markerField,
          // marked-line background (driven by RangeSet<Decoration>)
          EditorView.decorations.compute([markerField], (v) => markerLineDeco(v)),
          // marker gutter: stamp on each marked line; click toggles via store
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          (gutter(markerGutter as any) as any),
          cmSearch.search({ top: true }),
        ],
```

- [ ] **Step 3: Type-check + build**

```bash
pnpm exec vue-tsc --noEmit -p tsconfig.json
pnpm build 2>&1 | tail -30
```

Expected: 0 errors, build success. If `gutter` is reported as undefined, you forgot to add it to the destructure above.

- [ ] **Step 4: Commit**

```bash
git add src/composables/useParseEditor.ts
git commit -m "feat(parse): render 章 stamp in CM6 gutter for marked lines"
```

---


### Task 6: Search extension wiring + substring counter cursor

**Files:**
- Modify: `src/composables/useParseEditor.ts`

- [ ] **Step 1: Add search-cursor module-level state**

Inside the composable body (right after `markerLineDeco` declaration, before `mount`), add:

```ts
  // Substring-based hit cursor (mirrors the previous useChapterSearch semantics:
  // plain string contains, literal mode). Drives hitCount / currentHitIndex.
  // CM's search() extension handles its own highlighting in parallel via
  // setSearchQuery below; the substring cursor only feeds the toolbar counter.
  let currentQuery = '';
  let searchLines: string[] = [];
  let searchHits: number[] = [];
  let searchCursor = 0;

  function rebuildSearchIndex(doc: string): void {
    searchLines = doc.split('\n');
    searchHits = [];
    if (currentQuery) {
      for (let i = 0; i < searchLines.length; i++) {
        if (searchLines[i].includes(currentQuery)) searchHits.push(i);
      }
    }
    searchCursor = 0;
    hitCount.value = searchHits.length;
    currentHitIndex.value = searchHits.length === 0 ? 0 : 1;
  }
```

- [ ] **Step 2: Update `mount()` to call `rebuildSearchIndex` after view construction**

At the bottom of `mount()` (just before the closing `}` of the function), add one line:

```ts
    rebuildSearchIndex(doc);
```

- [ ] **Step 3: Update `replaceDoc` to also rebuild the search index**

Replace the function body with:

```ts
  function replaceDoc(text: string): void {
    const v = view.value;
    if (!v) return;
    v.dispatch({
      changes: { from: 0, to: v.state.doc.length, insert: text },
    });
    rebuildSearchIndex(text);
  }
```

- [ ] **Step 4: Implement `runSearch` and drive CM's internal highlight**

Replace the `runSearch` body in the returned object. The composable already destructured `cmSearch` from `@codemirror/search` inside `mount()`; reuse the in-scope reference rather than re-importing.

```ts
    runSearch: (query: string) => {
      const v = view.value;
      if (!v) return;
      currentQuery = query;
      rebuildSearchIndex(v.state.doc.toString());
      // Drive CM's internal highlight via setSearchQuery. Empty clears.
      v.dispatch({
        effects: cmSearch.setSearchQuery.of(new cmSearch.SearchQuery({ search: query })),
      });
    },
```

- [ ] **Step 5: Implement `nextHit` and `prevHit`**

Replace both bodies. `cmSearch` is the namespace import of `@codemirror/search` already in scope at composable level; reuse it.

```ts
    nextHit: () => {
      const v = view.value;
      if (!v || searchHits.length === 0) return;
      searchCursor = (searchCursor + 1) % searchHits.length;
      currentHitIndex.value = searchCursor + 1;
      const pos = v.state.doc.line(searchHits[searchCursor] + 1).from;
      v.dispatch({
        selection: { anchor: pos },
        effects: EditorView.scrollIntoView(pos, { y: 'start' }),
      });
      // Move CM's internal match-selection forward too, for visual parity.
      cmSearch.findNext(v);
    },
    prevHit: () => {
      const v = view.value;
      if (!v || searchHits.length === 0) return;
      searchCursor = (searchCursor - 1 + searchHits.length) % searchHits.length;
      currentHitIndex.value = searchCursor + 1;
      const pos = v.state.doc.line(searchHits[searchCursor] + 1).from;
      v.dispatch({
        selection: { anchor: pos },
        effects: EditorView.scrollIntoView(pos, { y: 'start' }),
      });
      cmSearch.findPrevious(v);
    },
```

- [ ] **Step 6: Type-check, build, tests**

```bash
pnpm exec vue-tsc --noEmit -p tsconfig.json
pnpm build 2>&1 | tail -30
pnpm test
```

Expected: 0 errors, build success, existing tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/composables/useParseEditor.ts
git commit -m "feat(parse): wire CM6 search extension and substring hit-cursor"
```

---

### Task 7: `scrollToLine` public helper

**Files:**
- Modify: `src/composables/useParseEditor.ts`

- [ ] **Step 1: Replace the noop body**

Replace the `scrollToLine` body in the returned object:

```ts
    scrollToLine: (line0based: number) => {
      const v = view.value;
      if (!v) return;
      // store is 0-based; CM6 is 1-based. Clamp to doc bounds.
      const safe = Math.max(1, Math.min(line0based + 1, v.state.doc.lines));
      const pos = v.state.doc.line(safe).from;
      v.dispatch({
        selection: { anchor: pos },
        effects: EditorView.scrollIntoView(pos, { y: 'start' }),
      });
    },
```

- [ ] **Step 2: Type-check**

```bash
pnpm exec vue-tsc --noEmit -p tsconfig.json
```

Expected: 0 errors.

- [ ] **Step 3: Commit**

```bash
git add src/composables/useParseEditor.ts
git commit -m "feat(parse): expose scrollToLine helper for chapter-click jump"
```

---


### Task 8: `parse.vue` — migrate right pane to CM6 (PART A: template + script changes)

**Files:**
- Modify: `src/views/parse.vue`

This is a coordinated migration touching template right pane, script imports, script composable wiring, and one `<style>` block. Steps 1–6 here; Steps 7–11 (lifecycle ordering + style additions) continue in Task 8B.

- [ ] **Step 1: Replace the right pane template block**

In `src/views/parse.vue`, find the `<div class="pane">` whose child is `<div class="pane-title">原文</div>`. Replace that entire pane (its outer `<div class="pane">…</div>` inclusive) with:

```html
      <div class="pane">
        <div class="pane-title">原文</div>
        <div class="search-toolbar">
          <input
            class="search-input"
            placeholder="全文搜索"
            :value="searchQuery"
            @input="onSearchInput(($event.target as HTMLInputElement).value)"
          />
          <span class="search-counter">{{ counterText }}</span>
          <Button size="small" :disabled="hitCount === 0" @click="onPrevHit">‹</Button>
          <Button size="small" :disabled="hitCount === 0" @click="onNextHit">›</Button>
        </div>
        <div ref="cmHost" class="cm-host" />
      </div>
```

- [ ] **Step 2: Drop `RecycleScroller` from the import**

In the `<script setup lang="ts">` block, locate:

```ts
import { DynamicScroller, DynamicScrollerItem, RecycleScroller } from 'vue-virtual-scroller';
import 'vue-virtual-scroller/dist/vue-virtual-scroller.css';
```

Replace with:

```ts
import { DynamicScroller, DynamicScrollerItem } from 'vue-virtual-scroller';
import 'vue-virtual-scroller/dist/vue-virtual-scroller.css';
```

(The CSS import stays because the left pane's `<DynamicScroller>` still needs the reset rules.)

- [ ] **Step 3: Replace `useChapterSearch` import + state with composable wiring**

Find the lines that begin with `import { useChapterSearch } from '../composables/useChapterSearch';` and the search composable instantiation. Replace ALL of:

```ts
import { useChapterSearch } from '../composables/useChapterSearch';
…
const search = useChapterSearch(searchQuery, () => store.rawLines);
const { hitLineIndices, hitCount, currentHitLineIndex, next, prev } = search;
const hitLineIndicesSet = computed(() => new Set(hitLineIndices.value));
```

with:

```ts
import { useParseEditor } from '../composables/useParseEditor';
…
const cmHost = ref<HTMLDivElement | null>(null);
const cmEditor = useParseEditor({
  host: cmHost,
  onMarkerToggle: (line1based) => {
    const key = String(line1based - 1); // CM6 1-based → store 0-based
    if (markerSet.value.has(key)) store.removeMarker(key);
    else store.addMarker(key);
  },
});
const hitCount = computed(() => cmEditor.hitCount.value);
const currentHitIndex = computed(() => cmEditor.currentHitIndex.value);
```

- [ ] **Step 4: Update `counterText` to use the new shape**

Replace the existing `counterText` body with:

```ts
const counterText = computed(() => {
  const total = hitCount.value;
  if (total === 0) return '0 / 0';
  return `${currentHitIndex.value} / ${total}`;
});
```

- [ ] **Step 5: Drop `scrollToActiveHit` body (composable handles scroll)**

Replace the function body:

```ts
function scrollToActiveHit() {
  // composable's nextHit/prevHit already scrollIntoView; nothing to do here.
}
```

- [ ] **Step 6: Reimplement search/chapter handlers**

Replace `onNextHit`, `onPrevHit`, `onSearchInput`, `onChapterClick`, and `onMarkLine` with:

```ts
function onNextHit() { cmEditor.nextHit(); }
function onPrevHit() { cmEditor.prevHit(); }

function onSearchInput(value: string) {
  searchQuery.value = value;
  cmEditor.runSearch(value);
}

function onChapterClick(item: ChapterSegment) {
  const line = store.startLineOf(item);
  if (line < 0) return;
  void nextTick(() => { cmEditor.scrollToLine(line); });
}

function onMarkLine(lineKey: string) {
  if (markerSet.value.has(lineKey)) store.removeMarker(lineKey);
  else store.addMarker(lineKey);
}
```

`onMarkLine` is retained for parity with any leftover bindings; if unused after migration the implementer may delete the helper.

Stop here. Continue with **Task 8B** for lifecycle ordering + style additions.

---


### Task 8B: `parse.vue` — lifecycle ordering + mount hook + style additions (PART B)

**Files:**
- Modify: `src/views/parse.vue`

- [ ] **Step 7: Add a marker-sync watcher**

After the existing `watch(() => Number(route.params.uploadId), …)` block, add:

```ts
watch(
  () => store.markers,
  (markers) => {
    cmEditor.setMarkers(new Set(markers.map((m) => Number(m))));
  },
  { deep: false },
);
```

- [ ] **Step 8: Trigger `cmEditor.mount` once `rawText` lands**

Inside the existing `watch(() => Number(route.params.uploadId), …)` body, after the `void store.load(id);` line, add:

```ts
  void nextTick(() => {
    const text = store.rawText;
    if (text) {
      void cmEditor.mount(text);
      cmEditor.setMarkers(new Set(store.markers.map((m) => Number(m))));
    }
  });
```

If `rawText` is empty at first paint (store still loading), repeat calls to `cmEditor.mount` on the same host remain safe — the composable's `mount` function destroys the prior `view.value` first. For the typical parse-page entry case, `rawText` is present immediately on `store.load`.

- [ ] **Step 9: Reorder `onUnmounted`**

Replace `onUnmounted(() => { store.unload(); });` with:

```ts
onUnmounted(() => {
  cmEditor.destroy();
  store.unload();
});
```

- [ ] **Step 10: Add CSS theme additions for the editor host + gutter**

In the `<style scoped>` block at the bottom of `parse.vue`, add the following rules (place after the existing `.scroller { … }` rule). Do NOT touch the existing chapter-list / search-toolbar rules.

```css
.cm-host {
  flex: 1;
  min-height: 0;
  border-top: 1px solid var(--border-color);
  overflow: hidden;
}
.cm-host .cm-editor {
  height: 100%;
}
/* Marked-line background (driven by RangeSet<Decoration>). */
.cm-marker-line {
  background-color: var(--bg-hover);
}
/* Marker gutter column — flex-centered for the stamp. */
.cm-gutter.cm-marker-stamp {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  background: transparent;
  cursor: default;
}
.cm-marker-stamp {
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
  letter-spacing: 0;
  transition: background 0.1s, color 0.1s;
}
.cm-marker-stamp:hover {
  background: var(--color-cinnabar);
  color: #faf6ee;
}
```

- [ ] **Step 11: Remove now-dead CSS classes**

The following CSS rules become dead after right-pane migration (per-line button rendering is gone). Remove them in the same commit:

```css
/* delete these blocks */
.line-row { … }
.line-row.marked { … }
.line-row.hit { … }
.line-row.active-hit { … }
.line-no { … }
.line-text { … }
```

- [ ] **Step 12: Type-check, build, run tests**

```bash
pnpm exec vue-tsc --noEmit -p tsconfig.json
pnpm build 2>&1 | tail -30
pnpm test
```

Expected: 0 errors, build success, existing tests pass.

- [ ] **Step 13: Commit**

```bash
git add src/views/parse.vue
git commit -m "refactor(parse): migrate right pane to CM6 EditorView"
```

---


### Task 9: Delete `useChapterSearch.ts` and its spec placeholder

**Files:**
- Delete: `src/composables/useChapterSearch.ts`
- Delete: `src/__tests__/useChapterSearch.spec.ts`

- [ ] **Step 1: Delete the files**

```bash
git rm src/composables/useChapterSearch.ts src/__tests__/useChapterSearch.spec.ts
```

- [ ] **Step 2: Confirm no stragglers**

```bash
rg -n 'useChapterSearch' src
```

Expected: matches only in `src/utils/format.ts` and `src/utils/status-locale.ts` (cross-reference comments; cleaned in Task 11). Zero matches in `src/views/parse.vue`.

- [ ] **Step 3: Verify build still green**

```bash
pnpm build 2>&1 | tail -10
```

Expected: success.

- [ ] **Step 4: Commit**

```bash
git commit -m "chore(parse): remove retired useChapterSearch composable and placeholder test"
```

---

### Task 10: Delete `MarkerButton.vue` (sole consumer was `parse.vue`)

**Files:**
- Delete: `src/components/MarkerButton.vue`

- [ ] **Step 1: Confirm sole consumer was `parse.vue`**

```bash
rg -n 'MarkerButton' src
```

Expected: no matches outside `src/components/MarkerButton.vue` itself. If any match remains, do not delete; investigate.

- [ ] **Step 2: Delete**

```bash
git rm src/components/MarkerButton.vue
```

- [ ] **Step 3: Verify build still green**

```bash
pnpm build 2>&1 | tail -10
```

Expected: success.

- [ ] **Step 4: Commit**

```bash
git commit -m "chore(parse): remove MarkerButton.vue (stamp moved into CM6 gutter)"
```

---

### Task 11: Clean `useChapterSearch` cross-references in comments

**Files:**
- Modify: `src/utils/format.ts`
- Modify: `src/utils/status-locale.ts`

- [ ] **Step 1: Inspect each match**

```bash
rg -n 'useChapterSearch' src/utils
```

Each line is a doc-comment cross-reference. Open each file in an editor and remove the cross-reference (typically a parenthetical `(参见 useChapterSearch.ts)`); do not rewrite surrounding prose.

- [ ] **Step 2: Confirm zero remaining matches**

```bash
rg -n 'useChapterSearch' src docs scripts src-tauri crates migrations
```

Expected: 0 matches.

- [ ] **Step 3: Verify type-check + tests**

```bash
pnpm exec vue-tsc --noEmit -p tsconfig.json && pnpm test
```

Expected: 0 errors, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/utils/format.ts src/utils/status-locale.ts
git commit -m "docs(utils): remove stale useChapterSearch cross-references"
```

---

### Task 12: Final verification — build, tests, manual smoke

**Files:** none

- [ ] **Step 1: Type-check, build, test**

```bash
pnpm exec vue-tsc --noEmit -p tsconfig.json && pnpm build && pnpm test
```

Expected: 0 errors from vue-tsc, build success, all existing vitest specs pass.

- [ ] **Step 2: Manual smoke checklist (run `pnpm dev`, open `/library/upload/<id>/parse`)**

Verify each item — do NOT proceed until all pass:

- [ ] **S1** — Right pane renders the novel text with **long lines wrapped**, not ellipsized.
- [ ] **S2** — Line-number gutter visible on the left of every line (CM6 built-in `lineNumbers()`).
- [ ] **S3** — Clicking a chapter in the left list scrolls the right pane to the chapter's start line. Selection visibly placed there.
- [ ] **S4** — Search input + counter + prev/next buttons work. Hit count `n / m` updates as you type. Prev/next cycle through matches.
- [ ] **S5** — For a marked line: red "章" stamp appears in the gutter; marked-line background (`var(--bg-hover)`) visible across the entire wrapped line(s).
- [ ] **S6** — Clicking a stamp toggles the marker; the left chapter list updates accordingly when the splitter re-derives segments.
- [ ] **S7** — Switching routes away and back to `/library/upload/<id>/parse` shows the editor remounts cleanly with no memory growth in DevTools.
- [ ] **S8** — Open a 10+ MB upload. First render takes a few seconds (acceptable; same UX as `Upload.vue`). After mount, interactions feel responsive.

> Failure triage:
> - S1 fails → Step 1 of Task 3 (forgot `EditorView.lineWrapping`).
> - S2 fails → Step 1 of Task 3 (forgot `lineNumbers()` in extensions).
> - S3 fails → Task 7 (`scrollToLine` math) or Step 6 of Task 8 (`onChapterClick`).
> - S4 fails → Task 6 (search wiring).
> - S5/S6 fail → Task 4 (StateField) or Task 5 (gutter widget).
> - S7 fails → Step 9 of Task 8B (unmount order) or Task 7 (`destroy()`).

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "test(parse): confirm manual smoke for CM6 migration"
```

---

## Self-Review Checklist

Run before considering this plan executed:

1. **Spec coverage:**
   - §2 Goal 1 (long lines wrap) → Task 3 (`EditorView.lineWrapping`).
   - §2 Goal 2 (10+ MB) → Task 8B Step 8 (lazy chunk + mount).
   - §2 Goal 3 (marker toggle) → Tasks 4, 5, 8 Step 6.
   - §2 Goal 4 (full-text search) → Task 6.
   - §2 Goal 5 (jump-to-chapter) → Tasks 7 + 8 Step 6.
   - §5.5 lifecycle / destroy order → Task 8B Step 9.
   - §5.6 theme → Tasks 3 + 8B Step 10.
   - §6 risks (off-by-one, RangeSetBuilder, gutter click, etc.) → Tasks 4, 5, 7.
   - §4 file table → Tasks 1, 8 (template), 9, 10, 11.

2. **Placeholders:** none — every code-modifying step contains a complete, runnable code block. The `void X; void Y;` in Task 4 marks unused-but-intentional locals explicitly; engineer leaves them as-is.

3. **API consistency:**
   - `markerField` defined in Task 4, referenced in Task 5 and Task 8 — consistent symbol.
   - `markerLineDeco` defined in Task 4, referenced in Task 8B Step 10 (`EditorView.decorations.compute(...)`) — consistent.
   - `markerGutter` defined in Task 5, referenced in Task 8B Step 2 (`cmView.gutter(markerGutter ...)`) — consistent.
   - `cmSearch.search({ top: true })` registered in Task 3 (so CM imports include the extension); Task 6 wires run/next/prev against the same namespace.
   - Public composable API (`mount / setMarkers / scrollToLine / runSearch / nextHit / prevHit / replaceDoc / destroy`) consistent across Task 2 (skeleton), Task 3 (`mount` body + `replaceDoc`), Tasks 4 (setMarkers), 5 (gutter, no public surface change), 6 (run/next/prev), 7 (scrollToLine).

4. **Commit count:** 13 implementation commits (one per task; Tasks 8 and 8B are one commit). Acceptable for diff size.
