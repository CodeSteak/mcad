// Shared, non-reactive objects between editor, view and engine client: the editor view (for
// cursor jumps), the version mapper (old scene positions → current document) and cached
// scene indices.

import { Text } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import type { Scene } from "../engine/protocol";
import { SceneIndex } from "../logic/sceneIndex";
import { VersionMapper } from "../logic/positions";
import { store } from "./store";

class Bridge {
  view: EditorView | null = null;
  mapper = new VersionMapper(0);
  /** performance.now() of the last edit that produced a document version. */
  editTimes = new Map<number, number>();
  private indices = new WeakMap<Scene, { rev: number; idx: SceneIndex }>();

  doc(): Text {
    return this.view?.state.doc ?? Text.empty;
  }

  text(): string {
    return this.doc().toString();
  }

  /** Index of a scene with positions mapped into the current document. */
  index(scene: Scene): SceneIndex {
    const rev = store.getState().docRevision;
    const c = this.indices.get(scene);
    if (c && c.rev === rev) return c.idx;
    const idx = new SceneIndex(scene, this.mapper.mapper(scene.version));
    this.indices.set(scene, { rev, idx });
    return idx;
  }

  /** Moves the editor cursor (and scrolls it into view). */
  setCursor(pos: number, focus = true): void {
    const v = this.view;
    if (!v) return;
    const p = Math.max(0, Math.min(pos, v.state.doc.length));
    v.dispatch({ selection: { anchor: p }, scrollIntoView: true, userEvent: "select.view" });
    if (focus) v.focus();
  }

  setCursorLine(line: number, col = 0): void {
    const d = this.doc();
    if (line < 1 || line > d.lines) return;
    const l = d.line(line);
    this.setCursor(Math.min(l.from + col, l.to));
  }
}

export const bridge = new Bridge();
