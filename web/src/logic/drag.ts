// One step of a handle drag (Spec: Griffe, Regeln 4–6): mouse ray → DragTarget for the engine
// plus a preview (grip position, highlighted snap target). Pure; the drag controller sends.
//
// * Point handles: snap targets (grid points, anchors) within the snap radius on screen win;
//   otherwise the mouse is intersected with a drag plane and the numbers are written.
// * 1D handles (height, radius, plane, scale): the mouse is projected onto the handle line;
//   grid planes on that line are snap targets (`g.name`).
// * Rotation rings: the angle of the mouse on the ring plane (the engine rounds to the step).
// * Alt: no snapping, numbers only. A handle that is only a reference never becomes numbers
//   without Alt (it keeps the last reference).
// * Axis lock (Shift: axis from the mouse motion; keys X/Y/Z): a point handle moves along one
//   axis of its local system; only grid points on that axis are snap targets.

import type { DragTarget, Handle, SnapTarget, Vec3 } from "../engine/protocol";
import { add, apply, applyDir, scale, sub } from "./math";
import { type HandleGeom, type Ray, type ScreenPoint, nearestSnap, pointDragPlane, pureRef, rayLineParam, rayPlane, ringAngle, toLocal } from "./handles";
import { roundTo } from "./format";

export interface DragInput {
  geom: HandleGeom;
  ray: Ray;
  mouse: ScreenPoint;
  viewDir: Vec3;
  alt: boolean;
  targets: SnapTarget[];
  /** Screen position of a context point (null if behind the camera). */
  project: (p: Vec3) => ScreenPoint | null;
  snapRadius: number;
  /** Previous angle (rings) for unwrapping. */
  prevAngle: number;
  /** Rounding steps used for the preview. */
  stepMm: number;
  stepDeg: number;
  /** Axis-constrained point drag. */
  axis?: AxisLock | null;
}

export interface AxisLock {
  /** Axis of the handle's local system (0 = x, 1 = y, 2 = z). */
  axis: number;
  /** Local position at the start of the drag (the line goes through it). */
  origin: Vec3;
}

const UNIT: Vec3[] = [
  [1, 0, 0],
  [0, 1, 0],
  [0, 0, 1],
];

/** Axes a point handle can be locked to (2D points: x and y). */
export function lockableAxes(h: Handle): number[] {
  if (h.kind.kind !== "point") return [];
  return h.value.length === 2 ? [0, 1] : [0, 1, 2];
}

/**
 * Axis of the handle's local system whose screen direction best matches the mouse motion
 * (null below `minPx` of motion).
 */
export function axisFromMotion(h: Handle, origin: Vec3, from: { x: number; y: number }, to: { x: number; y: number }, project: (p: Vec3) => { x: number; y: number } | null, minPx = 6): number | null {
  const mx = to.x - from.x;
  const my = to.y - from.y;
  const ml = Math.hypot(mx, my);
  if (ml < minPx) return null;
  const o = apply(h.frame, origin);
  const s0 = project(o);
  if (!s0) return null;
  let best: number | null = null;
  let bestCos = -1;
  for (const k of lockableAxes(h)) {
    const d = applyDir(h.frame, UNIT[k]);
    const len = Math.hypot(d[0], d[1], d[2]) || 1;
    const s1 = project(add(o, scale(d, 1 / len)));
    if (!s1) continue;
    const sx = s1.x - s0.x;
    const sy = s1.y - s0.y;
    const sl = Math.hypot(sx, sy);
    if (sl < 1e-6) continue;
    const c = Math.abs((sx * mx + sy * my) / (sl * ml));
    if (c > bestCos) {
      bestCos = c;
      best = k;
    }
  }
  return best;
}

/** Guide line of an axis lock (context coordinates), `len` to both sides of the start. */
export function axisLine(h: Handle, lock: AxisLock, len: number): { from: Vec3; to: Vec3 } {
  const o = apply(h.frame, lock.origin);
  const d = applyDir(h.frame, UNIT[lock.axis]);
  const l = Math.hypot(d[0], d[1], d[2]) || 1;
  const u = scale(d, len / l);
  return { from: sub(o, u), to: add(o, u) };
}

/** Snap target on the locked axis (other local components equal to the start). */
function onAxis(t: SnapTarget, lock: AxisLock, dims: number): boolean {
  for (let k = 0; k < dims; k++) {
    if (k === lock.axis) continue;
    if (Math.abs(t.local[k] - lock.origin[k]) > 1e-6 * Math.max(1, Math.abs(lock.origin[k]))) return false;
  }
  return true;
}

export interface DragStep {
  /** Message target, `null` = nothing to send. */
  target: DragTarget | null;
  /** Grip preview position (context coordinates). */
  grip: Vec3;
  snap: SnapTarget | null;
  /** New angle (rings), for the next step's unwrapping. */
  angle: number | null;
}

export function dragStep(d: DragInput): DragStep {
  const { geom } = d;
  const h = geom.handle;
  const snapHit = d.alt ? null : nearestSnap(d.targets, (i) => d.project(d.targets[i].world), d.mouse, d.snapRadius);

  if (geom.ring) {
    const a = ringAngle(d.ray, geom.ring, d.prevAngle);
    if (a === null) return { target: null, grip: geom.grip, snap: null, angle: null };
    const shown = roundTo(a, d.stepDeg);
    const r = geom.ring;
    const rad = (shown * Math.PI) / 180;
    const dir = add(scale(r.u, Math.cos(rad)), scale(r.v, Math.sin(rad)));
    return { target: { kind: "value", value: a }, grip: add(r.center, scale(dir, r.radius)), snap: null, angle: a };
  }

  if (geom.line) {
    const l = geom.line;
    const t = rayLineParam(d.ray, l.base, l.dir);
    if (snapHit) return { target: { kind: "snap", snap: snapHit.target.snap }, grip: snapHit.target.result_world, snap: snapHit.target, angle: null };
    if (t === null || (pureRef(h) && !d.alt)) return { target: null, grip: geom.grip, snap: null, angle: null };
    const step = h.kind.kind === "scale" ? 0 : d.stepMm;
    const shown = roundTo(t, step);
    return { target: { kind: "value", value: t }, grip: add(l.base, scale(l.dir, shown)), snap: null, angle: null };
  }

  // Point handle along one axis.
  const lock = d.axis && lockableAxes(h).includes(d.axis.axis) ? d.axis : null;
  if (lock) {
    const k = lock.axis;
    const cands = d.alt ? [] : d.targets.filter((t) => onAxis(t, lock, h.value.length));
    const hit = nearestSnap(cands, (i) => d.project(cands[i].world), d.mouse, d.snapRadius);
    if (hit) return { target: { kind: "snap", snap: hit.target.snap }, grip: hit.target.result_world, snap: hit.target, angle: null };
    if (pureRef(h) && !d.alt) return { target: null, grip: geom.grip, snap: null, angle: null };
    const base = apply(h.frame, lock.origin);
    const t = rayLineParam(d.ray, base, applyDir(h.frame, UNIT[k]));
    if (t === null) return { target: null, grip: geom.grip, snap: null, angle: null };
    const local: Vec3 = [...lock.origin];
    local[k] += t;
    const c = h.comps[k];
    const shown: Vec3 = [...lock.origin];
    shown[k] = c ? roundTo(local[k] * c.scale, d.stepMm) / c.scale : local[k];
    return { target: { kind: "point", pos: local }, grip: apply(h.frame, shown), snap: null, angle: null };
  }

  // Point handle.
  if (snapHit) return { target: { kind: "snap", snap: snapHit.target.snap }, grip: snapHit.target.result_world, snap: snapHit.target, angle: null };
  if (pureRef(h) && !d.alt) return { target: null, grip: geom.grip, snap: null, angle: null };
  const pl = pointDragPlane(h, geom.grip, d.viewDir);
  const w = rayPlane(d.ray, pl.p, pl.n);
  if (!w) return { target: null, grip: geom.grip, snap: null, angle: null };
  const local = toLocal(h, w);
  // Preview where the engine will put the grip: literal components rounded, others unchanged.
  const shown = [0, 1, 2].map((k) => {
    const c = h.comps[k];
    if (k >= h.value.length) return h.pos[k];
    if (!c) return h.pos[k];
    return roundTo(local[k] * c.scale, d.stepMm) / c.scale;
  }) as Vec3;
  return { target: { kind: "point", pos: local }, grip: apply(h.frame, shown), snap: null, angle: null };
}

/** Key of a drag target (identical consecutive targets are not sent again). */
export function targetKey(t: DragTarget | null): string {
  if (!t) return "";
  if (t.kind === "snap") return `s:${JSON.stringify(t.snap)}`;
  if (t.kind === "value") return `v:${t.value.toFixed(6)}`;
  return `p:${t.pos.map((x) => x.toFixed(6)).join(",")}`;
}

/**
 * At most one drag message in flight; the latest position wins (Spec: ruckelfrei).
 * `send` is called with the next target when nothing is in flight.
 */
export class DragQueue {
  private inFlight = false;
  private pending: DragTarget | null = null;
  private lastKey = "";
  sent = 0;
  dropped = 0;

  constructor(private send: (t: DragTarget) => void) {}

  /** New position. Returns whether it was sent at once. */
  push(t: DragTarget | null): boolean {
    if (!t) return false;
    const k = targetKey(t);
    if (k === this.lastKey && !this.pending) return false;
    if (this.inFlight) {
      if (this.pending) this.dropped++;
      this.pending = t;
      return false;
    }
    this.dispatch(t);
    return true;
  }

  /** The answer to the message in flight arrived. */
  answered(): void {
    this.inFlight = false;
    const p = this.pending;
    this.pending = null;
    if (p && targetKey(p) !== this.lastKey) this.dispatch(p);
  }

  get busy(): boolean {
    return this.inFlight || this.pending !== null;
  }

  private dispatch(t: DragTarget): void {
    this.inFlight = true;
    this.lastKey = targetKey(t);
    this.sent++;
    this.send(t);
  }
}
