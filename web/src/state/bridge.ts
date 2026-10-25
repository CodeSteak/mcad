// Shared, non-reactive objects between editor, view and engine client: the editor view (for
// cursor jumps), the version mapper (old scene positions → current document) and cached
// scene indices.

import { Text } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import type { Scene } from "../engine/protocol";
import { fileIds } from "../logic/files";
import { SceneIndex } from "../logic/sceneIndex";
import { VersionMapper } from "../logic/positions";
import { project } from "./project";
import { store } from "./store";

class Bridge {
  view: EditorView | null = null;
  mapper = new VersionMapper(0);
  /** performance.now() of the last edit that produced a document version. */
  editTimes = new Map<number, number>();
  /** Source version → performance.now() of the Drag message whose edits produced it. */
  dragTimes = new Map<number, number>();
  /** Last drag end with edits: time and source version (fine mesh latency). */
  dragEnd: { at: number; version: number } | null = null;
  /** Click on an assertion warning at the end of a line of the main file (runtime: jump and
   *  frame the violation); `true` if it handled the click. */
  onAssertClick: ((line: number) => boolean) | null = null;
  private indices = new WeakMap<Scene, { rev: number; file: string; idx: SceneIndex }>();

  /** Document in the editor (the active project file). */
  doc(): Text {
    return this.view?.state.doc ?? Text.empty;
  }

  /** Text in the editor (the active project file). */
  text(): string {
    return this.doc().toString();
  }

  /** Text of the main file (what is evaluated). */
  mainText(): string {
    return project.active === project.main ? this.text() : project.mainText();
  }

  /** Index of a scene with positions mapped into the current document (the active file). */
  index(scene: Scene): SceneIndex {
    const rev = store.getState().docRevision;
    const file = project.active;
    const c = this.indices.get(scene);
    if (c && c.rev === rev && c.file === file) return c.idx;
    const idx = new SceneIndex(scene, this.mapper.mapper(scene.version), fileIds(scene, file, project.main));
    this.indices.set(scene, { rev, file, idx });
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
