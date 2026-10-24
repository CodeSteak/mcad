// Sketch editor (Plan 17.6, 17.12): drawing mode of the 3D view. The engine does the work –
// which sketch, where the code goes, snapping, preview, the code of each action; this
// controller only forwards the mouse (ray, Alt, clicks) and applies the answers:
//
//   mouse move → `Sketch { ray }` (at most one in flight, the latest position wins)
//   click      → `Sketch { ray, click }` → points kept, or edits: one undo step, the cursor
//                after the new code, the source sent at once (the next request already sees it)
//   Esc        → drops the picked points; again: leaves the mode
//
// Entering looks perpendicularly onto the sketch plane (also under transformations and in
// rotate_extrude). Handle drags keep working (the view's grips win over the tool).

import { redo, undo } from "@codemirror/commands";
import type { EngineClient } from "../engine/client";
import type { Mat4, Ray, SketchAnswer, SketchRequest, SketchTool } from "../engine/protocol";
import { pixelSize, sketchTransaction } from "../logic/sketchEditor";
import { bridge } from "../state/bridge";
import { project } from "../state/project";
import { type SketchState, store } from "../state/store";

/** What the controller needs from the 3D view. */
export interface SketchView {
  ray(clientX: number, clientY: number): Ray;
  /** Vertical field of view (degrees) and viewport height (pixels). */
  fov(): { deg: number; height: number };
  /** Camera perpendicular onto the plane, framing `min`..`max` (plane coordinates). */
  lookAtPlane(plane: Mat4, min: [number, number], max: [number, number], withModel?: boolean): void;
}

interface Pending {
  ray: Ray | null;
  alt: boolean;
  /** `close` / `delete` instead of a click of the tool. */
  action?: "close" | "delete";
}

export class SketchController {
  private view: SketchView | null = null;
  private seq = 0;
  /** Request in flight: id and whether it was a click. */
  private inflight: { id: number; click: boolean; tool: SketchTool } | null = null;
  private clicks: Pending[] = [];
  private move: Pending | null = null;
  private infoWanted = false;
  /** Plane the camera was aligned to (re-aligned when the sketch plane changes). */
  private aligned: Mat4 | null = null;
  private listeners: Array<[string, EventListener]> = [];
  /** Answers (tests, report): count. */
  answers = 0;

  constructor(
    private client: EngineClient,
    private sendSource: () => void,
  ) {
    client.onSketch((a) => this.onAnswer(a));
  }

  setView(v: SketchView | null): void {
    this.view = v;
  }

  get active(): boolean {
    return store.getState().sketch !== null;
  }

  private state(): SketchState | null {
    return store.getState().sketch;
  }

  private patch(p: Partial<SketchState>): void {
    const s = this.state();
    if (s) store.setState({ sketch: { ...s, ...p } });
  }

  /** Enters the drawing mode (tool: line), looking onto the sketch plane at the cursor. */
  enter(tool: SketchTool = "line"): boolean {
    if (this.active) return true;
    if (project.active !== project.main) {
      store.setState({ notice: "Skizzen-Editor: nur in der Hauptdatei (Tab der Hauptdatei öffnen)" });
      return false;
    }
    this.aligned = null;
    this.clicks = [];
    this.move = null;
    store.setState({ sketch: { tool, points: [], answer: null, message: null, actions: 0 } });
    this.listen("keydown", (e) => this.onKey(e as KeyboardEvent));
    this.request(null, false, false, true);
    return true;
  }

  exit(): void {
    if (!this.active) return;
    for (const [t, f] of this.listeners) window.removeEventListener(t, f, true);
    this.listeners = [];
    this.clicks = [];
    this.move = null;
    store.setState({ sketch: null });
  }

  setTool(tool: SketchTool): void {
    if (!this.active) {
      this.enter(tool);
      return;
    }
    this.clicks = [];
    this.patch({ tool, points: [], message: null });
    this.request(null, false, false, false);
  }

  /** Mouse moved over the view (plane position, snap, preview). */
  hover(ray: Ray, alt: boolean): void {
    if (!this.active) return;
    this.move = { ray, alt };
    this.pump();
  }

  /** Mouse click in the view. */
  click(ray: Ray, alt: boolean): void {
    if (!this.active) return;
    this.clicks.push({ ray, alt });
    this.move = null;
    this.pump();
  }

  /** Esc: drop the picked points; without points leave the mode. */
  escape(): void {
    const s = this.state();
    if (!s) return;
    if (s.points.length) {
      this.clicks = [];
      this.patch({ points: [], message: null });
      this.request(null, false, false, false);
    } else this.exit();
  }

  /** `close();` for the open contour at the cursor. */
  closeContour(): void {
    this.action("close");
  }

  /** Removes the segment statement at the cursor. */
  deleteSegment(): void {
    this.action("delete");
  }

  private action(tool: "close" | "delete"): void {
    if (!this.active) return;
    this.clicks.push({ ray: null, alt: false, action: tool });
    this.pump();
  }

  /** Re-aligns the camera with the sketch plane. */
  align(): void {
    this.aligned = null;
    this.request(null, false, false, true);
  }

  private cursor(): number {
    const v = bridge.view;
    return v ? v.state.selection.main.head : store.getState().cursor;
  }

  private base(id: number, tool: SketchTool): SketchRequest {
    const s = this.state();
    const st = store.getState();
    const f = this.view?.fov() ?? { deg: 35, height: 800 };
    return {
      id,
      cursor: this.cursor(),
      tool,
      points: s?.points ?? [],
      pixel: pixelSize(f.deg, f.height),
      snap_px: st.settings.snapPx,
      free: false,
      click: false,
      info: false,
      ...(s?.answer?.angle !== null && s?.answer?.angle !== undefined ? { angle: s.answer.angle } : {}),
    };
  }

  private request(ray: Ray | null, alt: boolean, click: boolean, info: boolean, action?: "close" | "delete"): void {
    if (info) this.infoWanted = true;
    if (this.inflight) return;
    const s = this.state();
    if (!s) return;
    const id = ++this.seq;
    const tool = action ?? s.tool;
    const r: SketchRequest = { ...this.base(id, tool), free: alt, click, info: this.infoWanted && !action };
    if (ray) r.ray = ray;
    if (!action) this.infoWanted = false;
    this.inflight = { id, click, tool };
    this.client.post({ type: "Sketch", ...r });
  }

  private pump(): void {
    if (this.inflight || !this.active) return;
    const c = this.clicks.shift();
    if (c) return this.request(c.ray, c.alt, true, false, c.action);
    const m = this.move;
    if (m) {
      this.move = null;
      this.request(m.ray, m.alt, false, false);
    } else if (this.infoWanted) this.request(null, false, false, true);
  }

  private onAnswer(a: SketchAnswer): void {
    const f = this.inflight;
    if (!f || f.id !== a.id) return;
    this.inflight = null;
    this.answers++;
    const s = this.state();
    if (!s) return;
    if (f.click) {
      const applied = a.edits.length > 0 && this.apply(a);
      // the raster may change with the new code (a new sketch, other grids)
      if (applied) this.infoWanted = true;
      const keep = { ...a, preview: [], snap: null, guides: [] };
      this.patch({
        points: f.tool === "close" || f.tool === "delete" ? s.points : a.points,
        message: a.message,
        answer: { ...keep, raster: a.raster ?? s.answer?.raster ?? null },
        actions: s.actions + (applied ? 1 : 0),
      });
      if (a.message) store.setState({ notice: a.message });
    } else {
      this.patch({ answer: { ...a, raster: a.raster ?? s.answer?.raster ?? null }, message: null });
    }
    if (a.raster && this.view && !samePlane(this.aligned, a.plane)) {
      this.aligned = a.plane;
      this.view.lookAtPlane(a.plane, a.raster.min, a.raster.max, a.sketch === null);
    }
    this.pump();
  }

  /** Applies an answer's edits: one undo step, the cursor after the new code. */
  private apply(a: SketchAnswer): boolean {
    const v = bridge.view;
    if (!v || a.edits.some((e) => e.file)) return false;
    const tr = sketchTransaction(v.state, bridge.mapper, a.version, a.edits, a.select ? [a.select[0], a.select[1]] : null);
    if (!tr) return false;
    v.dispatch(v.state.update(tr));
    // at once: the next request is answered on the new text
    this.sendSource();
    return true;
  }

  private listen(type: string, f: EventListener): void {
    window.addEventListener(type, f, true);
    this.listeners.push([type, f]);
  }

  private onKey(e: KeyboardEvent): void {
    if (!this.active || store.getState().drag) return;
    const inEditor = (e.target as HTMLElement | null)?.closest?.(".cm-editor") != null;
    const inField = (e.target as HTMLElement | null)?.closest?.("input, select, textarea") != null;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      this.escape();
      return;
    }
    if (inEditor || inField) return;
    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      this.deleteSegment();
    } else if ((e.ctrlKey || e.metaKey) && (e.key === "z" || e.key === "Z" || e.key === "y")) {
      const v = bridge.view;
      if (!v) return;
      e.preventDefault();
      if (e.key === "y" || e.shiftKey) redo(v);
      else undo(v);
    }
  }
}

function samePlane(a: Mat4 | null, b: Mat4): boolean {
  return a !== null && a.every((x, i) => Math.abs(x - b[i]) < 1e-9);
}
