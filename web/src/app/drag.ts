// Handle drags in the view (Spec: Ansicht → Code, Griffe und Rückschreiben):
//
//   pointerdown on a grip → DragStart (engine answers SnapTargets, switches to coarse meshes)
//   pointermove → dragStep → Drag (at most one in flight, latest position wins)
//   Edits → applied to the editor as live edits of one undo group → SetSource at once
//   pointerup → last position, DragEnd → the undo group becomes one history event
//
// Escape cancels the drag and reverts its edits. Shift locks a point handle to the axis of its
// local system that matches the mouse motion; X, Y, Z choose the axis (again: free).

import type { EngineClient } from "../engine/client";
import type { DragTarget, Edit, SnapTarget, Vec3 } from "../engine/protocol";
import { mapEdits } from "../editor/applyEdits";
import { EditGroup } from "../editor/editGroup";
import { type AxisLock, DragQueue, axisFromMotion, axisLine, dragStep, lockableAxes } from "../logic/drag";
import { type HandleGeom, type Ray, type ScreenPoint, alignTargets, toLocal } from "../logic/handles";
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
  /** Axis lock: guide line through the start position. */
  axis?: { axis: number; from: Vec3; to: Vec3 } | null;
}

interface Active {
  geom: HandleGeom;
  view: DragView;
  queue: DragQueue;
  targets: SnapTarget[];
  mouse: ScreenPoint;
  alt: boolean;
  /** Shift held: axis from the mouse motion. */
  shift: boolean;
  /** Axis chosen with X/Y/Z (wins over Shift). */
  axisKey: number | null;
  /** Axis found from the motion since Shift was pressed. */
  autoAxis: number | null;
  /** Mouse position when Shift was pressed. */
  shiftFrom: ScreenPoint;
  /** Axis of the lock in effect and the local position the lock line goes through (the
   *  position the grip had when the lock started, so the other components keep their text). */
  lockedAxis: number | null;
  lockOrigin: Vec3 | null;
  /** Local position the grip was last shown at. */
  lastLocal: Vec3 | null;
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
  start(geom: HandleGeom, view: DragView, e: { clientX: number; clientY: number; altKey: boolean; shiftKey?: boolean }): boolean {
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
      shift: !!e.shiftKey,
      axisKey: null,
      autoAxis: null,
      shiftFrom: { x: e.clientX, y: e.clientY },
      lockedAxis: null,
      lockOrigin: null,
      lastLocal: null,
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
    store.setState({ drag: { handle: id, snap: null, alt: e.altKey, edits: 0, axis: null }, viewHover: null });
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
    this.setShift(a, e.shiftKey);
    e.preventDefault();
    this.update();
  }

  private setShift(a: Active, down: boolean): void {
    if (down === a.shift) return;
    a.shift = down;
    a.autoAxis = null;
    a.shiftFrom = { ...a.mouse };
  }

  /** Axis lock in effect (point handles only). */
  private lock(a: Active): AxisLock | null {
    const h = a.geom.handle;
    const axes = lockableAxes(h);
    if (!axes.length || a.geom.line || a.geom.ring) return null;
    let axis = a.axisKey;
    if (axis === null && a.shift) {
      if (a.autoAxis === null) {
        a.autoAxis = axisFromMotion(h, h.pos, a.shiftFrom, a.mouse, (p) => {
          const q = a.view.project(p) as (ScreenPoint & { visible?: boolean }) | null;
          return q && q.visible !== false ? q : null;
        });
      }
      axis = a.autoAxis;
    }
    if (axis === null || !axes.includes(axis)) axis = null;
    if (axis !== a.lockedAxis) {
      a.lockedAxis = axis;
      a.lockOrigin = axis === null ? null : ([...(a.lastLocal ?? h.pos)] as Vec3);
    }
    return axis !== null && a.lockOrigin ? { axis, origin: a.lockOrigin } : null;
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
      return;
    }
    if (e.key === "Shift") {
      this.setShift(a, down);
      this.update();
      return;
    }
    const k = ["x", "y", "z"].indexOf(e.key.toLowerCase());
    if (k >= 0 && !e.ctrlKey && !e.metaKey) {
      // Keep the key away from the editor while dragging.
      e.preventDefault();
      e.stopPropagation();
      if (!down) return;
      if (!lockableAxes(a.geom.handle).includes(k)) return;
      a.axisKey = a.axisKey === k ? null : k;
      this.update();
    }
  }

  /** Computes the drag step for the current mouse position and queues it. */
  private update(): void {
    const a = this.a;
    if (!a) return;
    // Shift held, axis not yet known (too little motion): the grip waits instead of moving
    // freely (a free step would already change the other components).
    if (a.shift && a.axisKey === null && lockableAxes(a.geom.handle).length && !a.geom.line && !a.geom.ring && this.lock(a) === null) return;
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
      axis: this.lock(a),
    });
    if (step.angle !== null) a.angle = step.angle;
    if (step.target?.kind === "point" || step.target?.kind === "snap") a.lastLocal = toLocal(a.geom.handle, step.grip);
    a.lastStep = { grip: step.grip, snap: step.snap };
    a.queue.push(step.target);
    const lock = this.lock(a);
    let axis: DragPreview["axis"] = null;
    if (lock) {
      const h = a.geom.handle;
      const g0 = a.geom.grip;
      const len = Math.max(50, 4 * Math.hypot(step.grip[0] - g0[0], step.grip[1] - g0[1], step.grip[2] - g0[2]));
      axis = { axis: lock.axis, ...axisLine(h, lock, len) };
    }
    a.view.setDragPreview({ handle: a.geom.handle.id, grip: step.grip, targets: a.alt ? [] : a.targets, snap: step.snap, axis });
    const d = store.getState().drag;
    const snapText = step.snap?.text ?? null;
    const ax = lock?.axis ?? null;
    if (d && (d.snap !== snapText || d.alt !== a.alt || d.axis !== ax)) store.setState({ drag: { ...d, snap: snapText, alt: a.alt, axis: ax } });
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
