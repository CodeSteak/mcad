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
import { includeAt, resolvePath } from "../logic/files";
import { VersionMapper } from "../logic/positions";
import { bridge } from "../state/bridge";
import { project } from "../state/project";
import { store } from "../state/store";
import { buildDecorations } from "./decorations";
import { gitGutter } from "./gitGutter";
import { scad } from "./lang";

/** Forces the engine decorations to be rebuilt. */
export const refreshEffect = StateEffect.define<null>();

export interface EditorCallbacks {
  /** The document changed (new version). */
  onChange(text: string, version: number): void;
  onSave(): void;
  /** Ctrl+click on the path of `include <…>`/`use <…>`: open that project file. */
  onOpenFile?(path: string): void;
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
        activeFile: project.active,
        mainFile: project.main,
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

let callbacks: EditorCallbacks | null = null;

const hoverTracking = EditorView.domEventHandlers({
  mousemove(e, view) {
    const pos = view.posAtCoords({ x: e.clientX, y: e.clientY }, false);
    const line = view.state.doc.lineAt(pos);
    // Outside the text of the line (right of its end): still the line, not a reference. A
    // wrapped line ends on its last visual row; the rows above are text across the width.
    const coords = view.coordsAtPos(line.to);
    const inText = !coords || e.clientY < coords.top || e.clientX <= coords.right + 2;
    const p = inText ? pos : line.to;
    if (store.getState().editorHover !== p) store.setState({ editorHover: p });
  },
  mouseleave() {
    if (store.getState().editorHover !== null) store.setState({ editorHover: null });
  },
  // Ctrl/Cmd+click on a grid or anchor reference jumps to its declaration (a plain click
  // keeps placing the cursor so references stay editable).
  mousedown(e, view) {
    // click on an assertion warning at the line end: frame the violation in the view
    const el = e.target as HTMLElement | null;
    if (e.button === 0 && el?.closest?.(".cm-lineend-assert") && bridge.onAssertClick) {
      const pos = view.posAtCoords({ x: e.clientX, y: e.clientY }, false);
      if (project.active === project.main && bridge.onAssertClick(view.state.doc.lineAt(pos).number)) {
        e.preventDefault();
        return true;
      }
    }
    // click on a kernel error with a place (operands touching): frame the place
    if (e.button === 0 && el?.closest?.(".cm-lineend-locate") && bridge.onLocateClick) {
      const pos = view.posAtCoords({ x: e.clientX, y: e.clientY }, false);
      if (bridge.onLocateClick(view.state.doc.lineAt(pos).number)) {
        e.preventDefault();
        return true;
      }
    }
    if (e.button !== 0 || !(e.ctrlKey || e.metaKey)) return false;
    const pos = view.posAtCoords({ x: e.clientX, y: e.clientY }, false);
    // `include <datei>` / `use <datei>`: open the file (paths relative to this file).
    const inc = includeAt(view.state.doc.toString(), pos);
    if (inc && callbacks?.onOpenFile) {
      e.preventDefault();
      callbacks.onOpenFile(resolvePath(project.active, inc.path));
      return true;
    }
    const s = store.getState().scene;
    if (!s) return false;
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

/** A new editor state with all extensions (one per project file: own undo history). */
export function makeState(doc: string): EditorState {
  if (!callbacks) throw new Error("editor not created");
  return EditorState.create({ doc, extensions: editorExtensions(callbacks) });
}

function editorExtensions(cb: EditorCallbacks): Extension[] {
  return [
    lineNumbers(),
    gitGutter(),
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
  ];
}

export function createEditor(parent: HTMLElement, doc: string, cb: EditorCallbacks): EditorView {
  callbacks = cb;
  const state = makeState(doc);
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
