# Chapter Parse Page — Right Pane CodeMirror 6 Migration

**Status:** Awaiting user review
**Date:** 2026-08-23
**Scope:** `src/views/parse.vue` right pane only — replace `RecycleScroller`-based renderer with a CodeMirror 6 read-only editor.

---

# 1. Context

The current right pane of the chapter parse page uses `vue-virtual-scroller`'s `RecycleScroller` with a fixed `:item-size="24"`. The CSS for `.line-text` is `white-space: nowrap; overflow: hidden; text-overflow: ellipsis;`, which clips any line whose natural width exceeds the viewport. For Chinese novels in the 10+ MB range (≈ 300–500 万字, 10–30 万行), long paragraphs that should naturally wrap become unreadable — the parse UI today only displays an ellipsized first 24-px slice of each line.

The project already uses CodeMirror 6 in two other places:

- `src/views/Upload.vue` — editable (`054a34e perf(editor): 上传原文 textarea -> CodeMirror 6 虚拟化渲染`).
- `src/components/CleaningDialog.vue` — read-only (`ce7d27d perf(editor): 清洗预览 textarea -> 只读 CodeMirror 6`).

Both follow the same pattern (dynamic import, ref div, `EditorView.theme` over CSS variables, `cmView.destroy()` on unmount). The migration pattern is established; only the parse page lags. Earlier `RecycleScroller` work was committed as `b5b5baf refactor(store): 把搜索状态从 chapters store 抽到 parse.vue 局部` and `57670fc fix(parse): useChapterSearch 接受 MaybeRefOrGetter,断绝裸数组喂 watch` — confirming `parse.vue` is the natural target.

# 2. Goals

1. **Long lines wrap** at viewport width — full content readable.
2. **10+ MB documents** load smoothly via CM6's viewport virtualization.
3. **Marker ("章") toggle** retains per-line affordance (gutter click).
4. **Full-text search** with next/prev navigation + counter, equivalent semantics to current.
5. **Jump-to-chapter** on left-list click retains line-precision positioning.

# 3. Non-goals

- Left chapter list (`DynamicScroller` + title `<input>` + "并入上一章" button) — unchanged.
- `chapters` Pinia store and IPC layer — unchanged.
- No new third-party themes; reuse project's CSS variables.
- No new e2e/unit test coverage (parse page is uncovered; `useChapterSearch` test is a placeholder).
- No global keyboard shortcuts, no theming rework, no toolbar redesign.

# 4. File Changes

| Change | Path | Notes |
|---|---|---|
| **ADD** | `src/composables/useParseEditor.ts` | CM6 lifecycle composable: lazy-import chunks, build read-only EditorView, expose marker gutter, search hooks, jump, lifecycle. |
| **MODIFY** | `src/views/parse.vue` | Replace right-pane `RecycleScroller` with `<div ref="cmHost" />`; wire markers, search, jump through composable. |
| **DELETE** | `src/composables/useChapterSearch.ts` | Sole consumer was `parse.vue`; tests are placeholder-only. |
| **DELETE** | `src/__tests__/useChapterSearch.spec.ts` | File body is `export {};` placeholder. |
| **DELETE** | `src/components/MarkerButton.vue` | Sole consumer was `parse.vue`; stamp rendering moves into composable's gutter. |
| **MODIFY** | `src/utils/format.ts` | Remove `useChapterSearch` comment cross-references. |
| **MODIFY** | `src/utils/status-locale.ts` | Remove `useChapterSearch` comment cross-references. |

**Preserved (not deleted):**

- `vue-virtual-scroller` dependency — `src/views/DataAsset.vue` still uses `RecycleScroller`.
- `src/types/vue-virtual-scroller.d.ts` shim — `DataAsset.vue` needs it.
- `src/__tests__/chapters.spec.ts` — unrelated, untouched.

# 5. Behavior Spec

## 5.1 Read-only contract

- `EditorState.readOnly.of(true)` — fully non-editable.
- `drawSelection()` — copy-paste remains.
- `EditorView.lineWrapping` — long lines wrap at viewport width.

## 5.2 Marker gutter

- `StateField<ReadonlySet<number>>` (keyed by 1-based line number, mirrored from store).
- `RangeSetBuilder<Decoration>` rebuild on marker change; outputs `Decoration.line({ attributes: { class: 'cm-marker-line' } })` per marked line.
- CM6 `gutter({ class: 'cm-marker-stamp', lineMarker(view, lineBlock), domEventHandlers })` extension renders the red seal "章" in the gutter of marked lines.
- Click on a gutter stamp → toggles `store.addMarker / removeMarker` with the 1-based line key. Store is the single source of truth; composable watches `store.markers` and pushes updated state via dispatch effect.

## 5.3 Full-text search

- Use `@codemirror/search`'s `search({ top: true })` extension. Panel UI is not invoked (no `openSearchPanel` binding, no keymap); CM's query state is driven externally via the documented `setSearchQuery` StateEffect.
- `setSearchQuery.of(query)` effect drives CM's internal highlight.
- Toolbar input + counter + prev/next buttons retained visually unchanged; drive `findPrevious` / `findNext` view commands from `@codemirror/search`.
- `hitCount` and current-index display via internal substring-cursor helper (matches current `useChapterSearch` semantics — plain string `includes`, literal mode).

## 5.4 Jump-to-chapter

`scrollToLine(line0based)` in composable:

```ts
const pos = view.state.doc.line(line0based + 1).from;
view.dispatch({
  selection: { anchor: pos },
  effects: EditorView.scrollIntoView(pos, { y: 'start' }),
});
```

Store's 0-based line numbers convert at this single point.

## 5.5 Lifecycle / mount contract

- `onMounted`: read `cmHost.value`, call `useParseEditor({ host, doc: rawText, onMarkerToggle(line1based) })`. Returns `{ setMarkers, scrollToLine, runSearch, nextHit, prevHit, replaceDoc, destroy, hitCount, currentHitIndex }`.
- When `rawText` content changes (currently no path triggers this, but defensively): `replaceDoc(newText)` instead of full remount.
- `onUnmounted` order, pinned by code: **first** call `destroy()` (CM view cleanup), **then** `store.unload()`.

## 5.6 Theme

- Reuse `Upload.vue` pattern: `EditorView.theme({ '&': {...}, '.cm-content': {...}, '.cm-scroller': {...} })`.
- For the gutter: `'.cm-gutters': { backgroundColor: 'transparent', borderRight: '1px solid var(--border-color)' }` (gutters ARE visible here, contrary to Upload.vue's `'display': 'none'`).
- New theme selectors: `.cm-gutter.cm-marker-stamp` (container), `.cm-marker-line` (marked line background).
- CSS variables used: `color-cinnabar`, `color-paper-mist`, `bg-hover`, `radius-pin`, `border-color`, `border-strong`, `text-primary`, `text-secondary`.
- Stamp visual 1:1 from `MarkerButton.vue` (22×22, red border, hover inverted) — ported into `.cm-marker-stamp` rules.

# 6. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Large doc mount (>10 MB) blocks first paint | Reuse `Upload.vue`'s loading placeholder; same UX as that page. |
| Marker `RangeSet` rebuild cost for 10K+ markers | Build via `new RangeSetBuilder<Decoration>()` single pass; `RangeSet` handles line ranges in O(markers) for viewport queries. |
| Coordinate off-by-one (CM6 1-based vs store 0-based) | Single conversion point: `state.doc.line(line0based + 1).from`. |
| Custom gutter DOM click handler | `gutter({ domEventHandlers: { click(view, lineView, event) { ... } } })`; reads `view.state.doc.lineAt(lineView.from).number` for the clicked line. |
| Removing `MarkerButton.vue` breaks other importers | `rg MarkerButton` shows `parse.vue` is the sole consumer — verified pre-removal in commit. |
| Removing `useChapterSearch.ts` breaks cross-references | Strip doc references in `format.ts` and `status-locale.ts` in same commit. |
| `@codemirror/search` adds ≈ 30 KB gzipped | Acceptable; same lazy-load pattern as `Upload.vue` chunks. |
| Route reuse (same component, different `uploadId`) | `onMounted` / `onUnmounted` already wire remount correctly; composable respects `destroy()` first. |

# 7. Out of Scope (explicit)

- Tauri / IPC / rust backend changes.
- `chapters` store API changes.
- Visual companion / new mockup work.
- Bulk reformatting or "housekeeping" the rest of `parse.vue`.
- Adding tests for `useParseEditor` (parse page is uncovered; existing test placeholders are intentional).
- A wide-scope refactor that touches `DataAsset.vue` or anywhere else using `vue-virtual-scroller`.

# 8. Acceptance

Functional:

- Right pane mounts CM6 editor with `lineWrapping`; long lines fully visible.
- Toolbar input + counter + prev/next buttons behave as before.
- Clicking a chapter on the left scrolls the right pane to its starting line.
- Clicking a stamp toggles the store marker; visual update is immediate.
- 10+ MB upload loads without UI freezing; large jumps don't lag.

Non-functional:

- Lazy chunk for `@codemirror/search` — initial parse page load does NOT pull search payload (Vite dynamic-import boundary).
- EditorView is destroyed on unmount (no leaked editor instance across route changes).
- Existing left chapter list styling/behavior unchanged.

# 9. Open Questions

None — design approved by user prior to writing this spec.
