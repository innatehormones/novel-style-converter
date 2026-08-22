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
    setMarkers: (_lines1based: ReadonlySet<number>) => { /* Task 4 */ },
    scrollToLine: (_line0based: number) => { /* Task 7 */ },
    runSearch: (_query: string) => { /* Task 6 */ },
    nextHit: () => { /* Task 6 */ },
    prevHit: () => { /* Task 6 */ },
    hitCount,
    currentHitIndex,
    replaceDoc: (_text: string) => { /* Task 3 */ },
    destroy,
    mount,
  } as unknown as UseParseEditorApi;
}
