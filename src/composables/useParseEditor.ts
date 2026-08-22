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
  // Module references to CM6 runtime modules, populated in mount().
  let cmSearchMod: typeof import('@codemirror/search') | null = null;
  let cmViewEditor: typeof import('@codemirror/view').EditorView | null = null;


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
      _cmSearch,
    ] = await Promise.all([
      import('@codemirror/state'),
      import('@codemirror/view'),
      import('@codemirror/commands'),
      import('@codemirror/search'),
    ]) as [
      typeof import('@codemirror/state'),
      typeof import('@codemirror/view'),
      typeof import('@codemirror/commands'),
      typeof import('@codemirror/search'),
    ];
    cmViewEditor = EditorView;
    cmSearchMod = _cmSearch;

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
          cmSearchMod!.search({ top: true }),
        ],
      }),
      parent: host,
    });
    rebuildSearchIndex(doc);
  }

  function replaceDoc(text: string): void {
    const v = view.value;
    if (!v) return;
    v.dispatch({
      changes: { from: 0, to: v.state.doc.length, insert: text },
    });
    rebuildSearchIndex(text);
  }

  onBeforeUnmount(destroy);

  return {
    view,
    setMarkers: (lines1based: ReadonlySet<number>) => {
      const v = view.value;
      if (!v) return;
      v.dispatch({ effects: markerEffect.of(new Set(lines1based)) });
    },
    scrollToLine: (line0based: number) => {
      const v = view.value;
      if (!v) return;
      // store is 0-based; CM6 is 1-based. Clamp to doc bounds.
      const safe = Math.max(1, Math.min(line0based + 1, v.state.doc.lines));
      const pos = v.state.doc.line(safe).from;
      v.dispatch({
        selection: { anchor: pos },
        effects: cmViewEditor!.scrollIntoView(pos, { y: 'start' }),
      });
    },
    runSearch: (query: string) => {
      const v = view.value;
      if (!v || !cmSearchMod) return;
      currentQuery = query;
      rebuildSearchIndex(v.state.doc.toString());
      // Drive CM's internal highlight via setSearchQuery. Empty clears.
      v.dispatch({
        effects: cmSearchMod.setSearchQuery.of(new cmSearchMod.SearchQuery({ search: query })),
      });
    },
    nextHit: () => {
      const v = view.value;
      if (!v || !cmSearchMod || !cmViewEditor || searchHits.length === 0) return;
      searchCursor = (searchCursor + 1) % searchHits.length;
      currentHitIndex.value = searchCursor + 1;
      const pos = v.state.doc.line(searchHits[searchCursor] + 1).from;
      v.dispatch({
        selection: { anchor: pos },
        effects: cmViewEditor!.scrollIntoView(pos, { y: 'start' }),
      });
      // Move CM's internal match-selection forward too, for visual parity.
      cmSearchMod.findNext(v);
    },
    prevHit: () => {
      const v = view.value;
      if (!v || !cmSearchMod || !cmViewEditor || searchHits.length === 0) return;
      searchCursor = (searchCursor - 1 + searchHits.length) % searchHits.length;
      currentHitIndex.value = searchCursor + 1;
      const pos = v.state.doc.line(searchHits[searchCursor] + 1).from;
      v.dispatch({
        selection: { anchor: pos },
        effects: cmViewEditor!.scrollIntoView(pos, { y: 'start' }),
      });
      cmSearchMod.findPrevious(v);
    },
    hitCount,
    currentHitIndex,
    replaceDoc,
    destroy,
    mount,
  } as unknown as UseParseEditorApi;
}
