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

import type { DragTarget, SnapTarget, Vec3 } from "../engine/protocol";
import { add, apply, scale } from "./math";
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
