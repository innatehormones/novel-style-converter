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
    setMarkers: (_lines1based: ReadonlySet<number>) => { /* Task 4 */ },
    scrollToLine: (_line0based: number) => { /* Task 7 */ },
    runSearch: (_query: string) => { /* Task 6 */ },
    nextHit: () => { /* Task 6 */ },
    prevHit: () => { /* Task 6 */ },
    hitCount,
    currentHitIndex,
    replaceDoc,
    destroy,
    mount,
  } as unknown as UseParseEditorApi;
}
