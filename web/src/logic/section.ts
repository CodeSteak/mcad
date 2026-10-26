// 3D section view: plane, convention, snapping and the matching CLI call.
//
// Convention (exactly as `mcad render --section`): the plane through `p` with normal `n`
// removes the half space `n` points into; what stays is (q − p) · n ≤ 0. `x=v` means
// p = (v, 0, 0), n = +x; `--section-flip` reverses n.

import type { Scene, Vec3 } from "../engine/protocol";
import { apply } from "./math";

export type SectionAxis = "x" | "y" | "z" | "free";

export interface SectionState {
  on: boolean;
  axis: SectionAxis;
  /** Point on the plane (context coordinates). */
  point: Vec3;
  /** Unit normal before `flip`. */
  normal: Vec3;
  flip: boolean;
  /** Snap target the plane sits on (`g.deckel`, anchor `k.oben`), or null. */
  snap: string | null;
}

export const SECTION_OFF: SectionState = { on: false, axis: "z", point: [0, 0, 0], normal: [0, 0, 1], flip: false, snap: null };

const AXES: Record<"x" | "y" | "z", Vec3> = { x: [1, 0, 0], y: [0, 1, 0], z: [0, 0, 1] };

export function dot(a: Vec3, b: Vec3): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

export function normalize(v: Vec3): Vec3 {
  const l = Math.hypot(v[0], v[1], v[2]);
  return l > 1e-12 ? [v[0] / l, v[1] / l, v[2] / l] : [0, 0, 1];
}

/** Effective normal (with flip): points into the removed half. */
export function effectiveNormal(s: SectionState): Vec3 {
  return s.flip ? [-s.normal[0], -s.normal[1], -s.normal[2]] : s.normal;
}

/** Signed position of the plane along its (unflipped) normal: p · n. */
export function offset(s: SectionState): number {
  return dot(s.point, s.normal);
}

/** The plane moved to offset `t` along its normal (the point stays on the normal line). */
export function withOffset(s: SectionState, t: number, snap: string | null = null): SectionState {
  const d = t - offset(s);
  const p: Vec3 = [s.point[0] + d * s.normal[0], s.point[1] + d * s.normal[1], s.point[2] + d * s.normal[2]];
  return { ...s, point: s.axis === "free" ? p : axisPoint(s.axis, t), snap };
}

function axisPoint(a: "x" | "y" | "z", v: number): Vec3 {
  const n = AXES[a];
  return [n[0] * v, n[1] * v, n[2] * v];
}

/** Plane along an axis at `value` (CLI `--section a=value`). */
export function axisSection(a: "x" | "y" | "z", value: number, flip = false): SectionState {
  return { on: true, axis: a, point: axisPoint(a, value), normal: AXES[a], flip, snap: null };
}

/** A free plane through `point` with normal `n` (e.g. the viewing direction). */
export function freeSection(point: Vec3, n: Vec3, flip = false): SectionState {
  return { on: true, axis: "free", point, normal: normalize(n), flip, snap: null };
}

/** q stays visible: (q − p) · n ≤ 0 (with the effective normal). */
export function keeps(s: SectionState, q: Vec3): boolean {
  const n = effectiveNormal(s);
  return dot([q[0] - s.point[0], q[1] - s.point[1], q[2] - s.point[2]], n) <= 1e-9;
}

function num(v: number): string {
  const r = Math.round(v * 1e4) / 1e4;
  const t = r.toFixed(4).replace(/0+$/, "").replace(/\.$/, "");
  return t === "-0" ? "0" : t;
}

/** The section part of a `mcad render` call. */
export function cliSection(s: SectionState): string {
  const n = effectiveNormal(s);
  for (const a of ["x", "y", "z"] as const) {
    const e = AXES[a];
    const d = dot(n, e);
    if (Math.abs(Math.abs(d) - 1) < 1e-9) {
      const v = dot(s.point, e);
      return d > 0 ? `--section ${a}=${num(v)}` : `--section ${a}=${num(v)} --section-flip`;
    }
  }
  return `--section-plane ${[...s.point, ...n].map(num).join(",")}`;
}

/** Camera of the view as CLI options (azimuth 0° = from the front −y, 90° = from +x). */
export function cliCamera(eyeDir: Vec3): { azimuth: number; elevation: number } {
  const d = normalize(eyeDir);
  const elevation = (Math.asin(Math.max(-1, Math.min(1, d[2]))) * 180) / Math.PI;
  let azimuth = (Math.atan2(d[0], -d[1]) * 180) / Math.PI;
  if (azimuth < 0) azimuth += 360;
  return { azimuth: Math.round(azimuth * 10) / 10, elevation: Math.round(elevation * 10) / 10 };
}

/** Full `mcad render` call for the current section and view. */
export function cliCommand(s: SectionState, file: string, module: string | null, eyeDir: Vec3 | null, output = "section.png"): string {
  const q = (t: string) => (/^[A-Za-z0-9_./:-]+$/.test(t) ? t : `'${t.replace(/'/g, "'\\''")}'`);
  const parts = ["mcad", "render", q(file), "-o", q(output)];
  if (module) parts.push("--module", q(module));
  parts.push(cliSection(s));
  if (eyeDir) {
    const c = cliCamera(eyeDir);
    parts.push("--azimuth", num(c.azimuth), "--elevation", num(c.elevation));
  }
  return parts.join(" ");
}

/** Offsets along the normal the plane can snap to: grid planes of the context and anchors. */
export function snapCandidates(scene: Scene, s: SectionState): Array<{ t: number; label: string }> {
  const n = s.normal;
  const out: Array<{ t: number; label: string }> = [];
  for (const g of scene.grids) {
    if (!g.in_context) continue;
    for (const pl of g.planes) {
      const e: Vec3 = [0, 0, 0];
      e[pl.axis] = 1;
      const o = apply(g.frame, [0, 0, 0]);
      const tip = apply(g.frame, e);
      const wn = normalize([tip[0] - o[0], tip[1] - o[1], tip[2] - o[2]]);
      if (Math.abs(Math.abs(dot(wn, n)) - 1) > 1e-6) continue;
      const q: Vec3 = [0, 0, 0];
      q[pl.axis] = pl.value;
      out.push({ t: dot(apply(g.frame, q), n), label: `${g.name}.${pl.name}` });
    }
  }
  for (const a of scene.anchors) {
    out.push({ t: dot(a.point, n), label: a.text ?? a.name });
  }
  return out;
}
