// Handle drags in the view (Spec: Ansicht → Code, Griffe und Rückschreiben):
//
//   pointerdown on a grip → DragStart (engine answers SnapTargets, switches to coarse meshes)
//   pointermove → dragStep → Drag (at most one in flight, latest position wins)
//   Edits → applied to the editor as live edits of one undo group → SetSource at once
//   pointerup → last position, DragEnd → the undo group becomes one history event
//
// Escape cancels the drag and reverts its edits.

import type { EngineClient } from "../engine/client";
import type { DragTarget, Edit, SnapTarget, Vec3 } from "../engine/protocol";
import { mapEdits } from "../editor/applyEdits";
import { EditGroup } from "../editor/editGroup";
import { DragQueue, dragStep } from "../logic/drag";
import { type HandleGeom, type Ray, type ScreenPoint, alignTargets } from "../logic/handles";
import { bridge } from "../state/bridge";
import { store } from "../state/store";

/** What the drag controller needs from the 3D view. */
export interface DragView {
  ray(clientX: number, clientY: number): Ray;
  viewDir(): Vec3;
  project(p: Vec3): ScreenPoint | null;
  setDragPreview(p: DragPreview | null): void;
}

export interface DragPreview {
  handle: number;
  grip: Vec3;
  targets: SnapTarget[];
  snap: SnapTarget | null;
}

interface Active {
  geom: HandleGeom;
  view: DragView;
  queue: DragQueue;
  targets: SnapTarget[];
  mouse: ScreenPoint;
  alt: boolean;
  angle: number;
  ending: boolean;
  startedAt: number;
  /** performance.now() of the Drag message in flight. */
  sentAt: number;
  frames: number;
  lastFrame: number;
  maxGap: number;
  raf: number;
  lastStep: { grip: Vec3; snap: SnapTarget | null } | null;
  /** Source version whose scene the next drag message waits for. */
  awaitVersion: number | null;
  unsubScene: (() => void) | null;
}

/** Longest wait for the scene of a drag edit before the next position is sent. */
const SCENE_WAIT_MS = 1500;

export class DragController {
  private a: Active | null = null;
  private group = new EditGroup(() => bridge.view, "input.drag");
  /** Answers still expected for a cancelled drag (dropped when they arrive). */
  private stale = 0;
  private sendSource: () => void;
  private listeners: Array<[string, EventListener]> = [];

  constructor(
    private client: EngineClient,
    sendSource: () => void,
  ) {
    this.sendSource = sendSource;
    client.onEdits((v, e) => this.onEdits(v, e));
    client.onSnapTargets((h, _v, t) => {
      if (this.a && this.a.geom.handle.id === h) {
        this.a.targets = alignTargets(this.a.geom, t);
        this.update();
      }
    });
  }

  get active(): boolean {
    return this.a !== null;
  }

  get handle(): number | null {
    return this.a?.geom.handle.id ?? null;
  }

  /** Starts a drag on a grip. Locked handles do not start a drag. */
  start(geom: HandleGeom, view: DragView, e: { clientX: number; clientY: number; altKey: boolean }): boolean {
    if (this.a || geom.handle.locked) return false;
    const id = geom.handle.id;
    this.group.begin();
    const queue = new DragQueue((t: DragTarget) => {
      if (this.a) this.a.sentAt = performance.now();
      this.client.post({ type: "Drag", handle: id, target: t });
    });
    const now = performance.now();
    this.a = {
      geom,
      view,
      queue,
      targets: [],
      mouse: { x: e.clientX, y: e.clientY },
      alt: e.altKey,
      angle: geom.ring?.value ?? 0,
      ending: false,
      startedAt: now,
      sentAt: now,
      frames: 0,
      lastFrame: now,
      maxGap: 0,
      raf: 0,
      lastStep: null,
      awaitVersion: null,
      unsubScene: null,
    };
    store.setState({ drag: { handle: id, snap: null, alt: e.altKey, edits: 0 }, viewHover: null });
    this.client.post({ type: "DragStart", handle: id });
    this.listen("pointermove", (ev) => this.onMove(ev as PointerEvent));
    this.listen("pointerup", (ev) => this.onUp(ev as PointerEvent));
    this.listen("keydown", (ev) => this.onKey(ev as KeyboardEvent, true));
    this.listen("keyup", (ev) => this.onKey(ev as KeyboardEvent, false));
    this.listen("blur", () => this.onUp(null));
    const tick = () => {
      const a = this.a;
      if (!a) return;
      const t = performance.now();
      a.frames++;
      a.maxGap = Math.max(a.maxGap, t - a.lastFrame);
      a.lastFrame = t;
      a.raf = requestAnimationFrame(tick);
    };
    this.a.raf = requestAnimationFrame(tick);
    view.setDragPreview({ handle: id, grip: geom.grip, targets: [], snap: null });
    return true;
  }

  private listen(type: string, f: EventListener): void {
    window.addEventListener(type, f, true);
    this.listeners.push([type, f]);
  }

  private unlisten(): void {
    for (const [t, f] of this.listeners) window.removeEventListener(t, f, true);
    this.listeners = [];
  }

  private onMove(e: PointerEvent): void {
    const a = this.a;
    if (!a || a.ending) return;
    a.mouse = { x: e.clientX, y: e.clientY };
    a.alt = e.altKey;
    e.preventDefault();
    this.update();
  }

  private onKey(e: KeyboardEvent, down: boolean): void {
    const a = this.a;
    if (!a) return;
    if (e.key === "Escape" && down) {
      e.preventDefault();
      this.cancel();
      return;
    }
    if (e.key === "Alt") {
      e.preventDefault();
      a.alt = down;
      this.update();
    }
  }

  /** Computes the drag step for the current mouse position and queues it. */
  private update(): void {
    const a = this.a;
    if (!a) return;
    const s = store.getState().settings;
    const step = dragStep({
      geom: a.geom,
      ray: a.view.ray(a.mouse.x, a.mouse.y),
      mouse: a.mouse,
      viewDir: a.view.viewDir(),
      alt: a.alt,
      targets: a.targets,
      project: (p) => {
        const q = a.view.project(p) as (ScreenPoint & { visible?: boolean }) | null;
        return q && q.visible !== false ? q : null;
      },
      snapRadius: s.snapPx,
      prevAngle: a.angle,
      stepMm: s.stepMm,
      stepDeg: s.stepDeg,
    });
    if (step.angle !== null) a.angle = step.angle;
    a.lastStep = { grip: step.grip, snap: step.snap };
    a.queue.push(step.target);
    a.view.setDragPreview({ handle: a.geom.handle.id, grip: step.grip, targets: a.alt ? [] : a.targets, snap: step.snap });
    const d = store.getState().drag;
    const snapText = step.snap?.text ?? null;
    if (d && (d.snap !== snapText || d.alt !== a.alt)) store.setState({ drag: { ...d, snap: snapText, alt: a.alt } });
  }

  private onEdits(version: number, edits: Edit[]): void {
    const a = this.a;
    if (!a) {
      if (this.stale > 0) {
        this.stale--;
        return;
      }
      this.applyOnce(version, edits);
      return;
    }
    const view = bridge.view;
    if (edits.length && view) {
      const changes = mapEdits(bridge.mapper, version, edits, view.state.doc.length);
      if (changes && this.group.apply(changes)) {
        // Send at once (no debounce): the next Drag must be computed on this text.
        this.sendSource();
        const v = store.getState().sentVersion;
        bridge.dragTimes.set(v, a.sentAt);
        const d = store.getState().drag;
        if (d) store.setState({ drag: { ...d, edits: this.group.edits } });
        // The next position waits for the scene of this text: a new source would abort the
        // running kernel job, and with every move the model would never be shown.
        this.awaitScene(a, v);
        return;
      }
    }
    this.release(a);
  }

  private awaitScene(a: Active, version: number): void {
    a.awaitVersion = version;
    const done = () => {
      const st = store.getState();
      if ((st.complete?.version ?? -1) >= version) return true;
      // Syntax errors: no scene follows.
      return st.diagnostics.version >= version && st.diagnostics.items.some((x) => x.source === "syntax" && x.severity === "error");
    };
    const check = () => {
      if (this.a !== a || a.awaitVersion !== version || !done()) return;
      this.release(a);
    };
    a.unsubScene?.();
    const unsub = store.subscribe(check);
    // Safety net: never block the drag for longer than this.
    const timer = setTimeout(() => {
      if (this.a === a && a.awaitVersion === version) this.release(a);
    }, SCENE_WAIT_MS);
    a.unsubScene = () => {
      unsub();
      clearTimeout(timer);
    };
    check();
  }

  /** The answer to the drag message in flight is complete: send the latest position. */
  private release(a: Active): void {
    a.awaitVersion = null;
    a.unsubScene?.();
    a.unsubScene = null;
    a.queue.answered();
    if (a.ending && !a.queue.busy) this.finish();
  }

  /** Edits outside a drag ("Als Standard übernehmen"): one undoable change. */
  applyOnce(version: number, edits: Edit[]): boolean {
    const view = bridge.view;
    if (!view || !edits.length) return false;
    const changes = mapEdits(bridge.mapper, version, edits, view.state.doc.length);
    if (!changes) return false;
    view.dispatch({ changes, userEvent: "input.engine" });
    this.sendSource();
    return true;
  }

  private onUp(e: PointerEvent | null): void {
    const a = this.a;
    if (!a || a.ending) return;
    if (e) {
      a.mouse = { x: e.clientX, y: e.clientY };
      a.alt = e.altKey;
      this.update();
    }
    a.ending = true;
    if (!a.queue.busy) this.finish();
  }

  private finish(): void {
    const a = this.a;
    if (!a) return;
    this.a = null;
    cancelAnimationFrame(a.raf);
    a.unsubScene?.();
    this.unlisten();
    this.client.post({ type: "DragEnd", handle: a.geom.handle.id });
    const edits = this.group.edits;
    this.group.end();
    const ms = performance.now() - a.startedAt;
    const st = store.getState();
    const fps = ms > 0 ? (a.frames * 1000) / ms : 0;
    bridge.dragEnd = edits > 0 ? { at: performance.now(), version: st.sentVersion } : null;
    store.setState({
      drag: null,
      metrics: {
        ...st.metrics,
        dragFps: [...st.metrics.dragFps, { fps, frames: a.frames, ms, maxGapMs: a.maxGap, sent: a.queue.sent, dropped: a.queue.dropped }].slice(-50),
      },
    });
    a.view.setDragPreview(null);
  }

  /** Escape: reverts the drag's edits. */
  cancel(): void {
    const a = this.a;
    if (!a) return;
    this.a = null;
    cancelAnimationFrame(a.raf);
    a.unsubScene?.();
    this.unlisten();
    // An Edits answer is still on its way (not when only the scene was awaited).
    if (a.queue.busy && a.awaitVersion === null) this.stale++;
    this.client.post({ type: "DragEnd", handle: a.geom.handle.id });
    this.group.cancel();
    this.sendSource();
    store.setState({ drag: null });
    a.view.setDragPreview(null);
  }
}
