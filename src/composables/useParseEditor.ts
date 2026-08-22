import { onBeforeUnmount, shallowRef, type Ref } from 'vue';
import type { EditorView as EditorViewType, DecorationSet as DecorationSetType } from '@codemirror/view';
import type { EditorState as EditorStateType } from '@codemirror/state';
import { StateField, StateEffect, RangeSetBuilder } from '@codemirror/state';
import { Decoration } from '@codemirror/view';

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

  // Marker StateField is built once per mount.
  const markerEffect = StateEffect.define<ReadonlySet<number>>();
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
  const markerLineDeco = (state: EditorStateType): DecorationSetType => {
    const set = state.field(markerField, false) ?? new Set<number>();
    const builder = new RangeSetBuilder<Decoration>();
    for (const line1based of set) {
      try {
        const line = state.doc.line(line1based);
        builder.add(line.from, line.from, Decoration.line({ attributes: { class: 'cm-marker-line' } }));
      } catch {
        // line out of range (e.g. doc shrunk); skip
      }
    }
    return builder.finish();
  };
  void markerLineDeco; // referenced in Task 5
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

  async function mount(doc: string): Promise<void> {
    const host = opts.host.value;
    if (!host) return;
    view.value?.destroy();
    view.value = null;

    const [
      { EditorState },
      { EditorView, drawSelection, lineNumbers, gutter },
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
          markerField,
          // marked-line background (driven by RangeSet<Decoration>)
          EditorView.decorations.compute([markerField], (v) => markerLineDeco(v)),
          // marker gutter: stamp on each marked line; click toggles via store
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          (gutter(markerGutter as any) as any),
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
    setMarkers: (lines1based: ReadonlySet<number>) => {
      const v = view.value;
      if (!v) return;
      v.dispatch({ effects: markerEffect.of(new Set(lines1based)) });
    },
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
