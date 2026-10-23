// CodeMirror 6 editor: language, keymaps, engine decorations and the sync with the store
// (document versions, cursor, hover).

import { autocompletion, closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, foldGutter, foldKeymap, indentOnInput, indentUnit } from "@codemirror/language";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { EditorState, StateEffect, type Extension } from "@codemirror/state";
import {
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
  crosshairCursor,
  drawSelection,
  dropCursor,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
} from "@codemirror/view";
import { refDeclaration } from "../logic/context";
import { VersionMapper } from "../logic/positions";
import { bridge } from "../state/bridge";
import { store } from "../state/store";
import { buildDecorations } from "./decorations";
import { scad } from "./lang";

/** Forces the engine decorations to be rebuilt. */
export const refreshEffect = StateEffect.define<null>();

export interface EditorCallbacks {
  /** The document changed (new version). */
  onChange(text: string, version: number): void;
  onSave(): void;
}

function hoverRefSpan(): { from: number; to: number } | null {
  const st = store.getState();
  const s = st.scene;
  if (!s || st.editorHover === null) return null;
  const idx = bridge.index(s);
  const r = idx.refAt(st.editorHover);
  return r === null ? null : idx.range(s.refs[r].range);
}

const engineDecorations = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = this.build(view);
    }
    update(u: ViewUpdate) {
      const refresh = u.transactions.some((t) => t.effects.some((e) => e.is(refreshEffect)));
      if (u.docChanged || refresh || u.viewportChanged) this.decorations = this.build(u.view);
    }
    build(view: EditorView): DecorationSet {
      const st = store.getState();
      const doc = view.state.doc;
      const hoverLine = st.editorHover !== null && st.editorHover <= doc.length ? doc.lineAt(st.editorHover).number : null;
      return buildDecorations(view.state, {
        scene: st.scene,
        complete: st.complete,
        diagnostics: st.diagnostics,
        mapper: bridge.mapper,
        engineError: st.engineError,
        viewHoverLines: st.viewHover?.lines ?? [],
        hoverLine,
        hoverRef: hoverRefSpan(),
      });
    }
  },
  { decorations: (v) => v.decorations },
);

function syncPlugin(cb: EditorCallbacks): Extension {
  return EditorView.updateListener.of((u) => {
    if (u.docChanged) {
      bridge.mapper.apply(u.changes);
      const st = store.getState();
      const version = st.docVersion + 1;
      store.setState({ docVersion: version, docRevision: st.docRevision + 1, cursor: u.state.selection.main.head, pendingSend: true });
      cb.onChange(u.state.doc.toString(), version);
    } else if (u.selectionSet) {
      store.setState({ cursor: u.state.selection.main.head });
    }
  });
}

const hoverTracking = EditorView.domEventHandlers({
  mousemove(e, view) {
    const pos = view.posAtCoords({ x: e.clientX, y: e.clientY }, false);
    const line = view.state.doc.lineAt(pos);
    // Outside the text of the line (right of its end): still the line, not a reference.
    const coords = view.coordsAtPos(line.to);
    const inText = !coords || e.clientX <= coords.right + 2;
    const p = inText ? pos : line.to;
    if (store.getState().editorHover !== p) store.setState({ editorHover: p });
  },
  mouseleave() {
    if (store.getState().editorHover !== null) store.setState({ editorHover: null });
  },
  // Ctrl/Cmd+click on a grid or anchor reference jumps to its declaration (a plain click
  // keeps placing the cursor so references stay editable).
  mousedown(e, view) {
    if (e.button !== 0 || !(e.ctrlKey || e.metaKey)) return false;
    const s = store.getState().scene;
    if (!s) return false;
    const pos = view.posAtCoords({ x: e.clientX, y: e.clientY }, false);
    const idx = bridge.index(s);
    const r = idx.refAt(pos);
    if (r === null) return false;
    const target = refDeclaration(s, r);
    if (!target) return false;
    e.preventDefault();
    bridge.setCursor(idx.range(target).from);
    return true;
  },
});

export function createEditor(parent: HTMLElement, doc: string, cb: EditorCallbacks): EditorView {
  const state = EditorState.create({
    doc,
    extensions: [
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      history(),
      foldGutter(),
      drawSelection(),
      dropCursor(),
      EditorState.allowMultipleSelections.of(true),
      indentOnInput(),
      indentUnit.of("    "),
      bracketMatching(),
      closeBrackets(),
      autocompletion({ activateOnTyping: false }),
      rectangularSelection(),
      crosshairCursor(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      EditorView.lineWrapping,
      keymap.of([
        { key: "Mod-s", preventDefault: true, run: () => (cb.onSave(), true) },
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        ...foldKeymap,
        indentWithTab,
      ]),
      scad(),
      engineDecorations,
      syncPlugin(cb),
      hoverTracking,
      EditorView.contentAttributes.of({ "aria-label": "Quelltext", spellcheck: "false" }),
    ],
  });
  const view = new EditorView({ state, parent });
  bridge.view = view;
  bridge.mapper = new VersionMapper(doc.length);
  // Rebuild decorations when engine data or hover state change.
  let queued = false;
  store.subscribe((s, p) => {
    if (
      s.scene === p.scene &&
      s.complete === p.complete &&
      s.diagnostics === p.diagnostics &&
      s.engineError === p.engineError &&
      s.viewHover === p.viewHover &&
      s.editorHover === p.editorHover
    )
      return;
    if (queued) return;
    queued = true;
    queueMicrotask(() => {
      queued = false;
      view.dispatch({ effects: refreshEffect.of(null) });
    });
  });
  return view;
}

/** Replaces the whole document (opening a file); one undo step. */
export function replaceDocument(view: EditorView, text: string): void {
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text }, selection: { anchor: 0 }, scrollIntoView: true });
}
