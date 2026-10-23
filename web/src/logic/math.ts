// Small helpers for the protocol's column-major 4×4 matrices and 3-vectors.

import type { Mat4, Vec3 } from "../engine/protocol";

export const IDENTITY: Mat4 = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];

export function mul(a: Mat4, b: Mat4): Mat4 {
  const o = new Array<number>(16);
  for (let c = 0; c < 4; c++) {
    for (let r = 0; r < 4; r++) {
      let s = 0;
      for (let k = 0; k < 4; k++) s += a[k * 4 + r] * b[c * 4 + k];
      o[c * 4 + r] = s;
    }
  }
  return o as Mat4;
}

export function apply(m: Mat4, p: Vec3): Vec3 {
  const [x, y, z] = p;
  const w = m[3] * x + m[7] * y + m[11] * z + m[15] || 1;
  return [
    (m[0] * x + m[4] * y + m[8] * z + m[12]) / w,
    (m[1] * x + m[5] * y + m[9] * z + m[13]) / w,
    (m[2] * x + m[6] * y + m[10] * z + m[14]) / w,
  ];
}

export function applyDir(m: Mat4, d: Vec3): Vec3 {
  const [x, y, z] = d;
  return [m[0] * x + m[4] * y + m[8] * z, m[1] * x + m[5] * y + m[9] * z, m[2] * x + m[6] * y + m[10] * z];
}

/** Inverse of an affine matrix (last row 0 0 0 1). */
export function invertAffine(m: Mat4): Mat4 {
  const [a, b, c, , d, e, f, , g, h, i] = m;
  const det = a * (e * i - f * h) - d * (b * i - c * h) + g * (b * f - c * e);
  if (Math.abs(det) < 1e-300) return IDENTITY;
  const inv = 1 / det;
  const r00 = (e * i - f * h) * inv;
  const r01 = (g * f - d * i) * inv;
  const r02 = (d * h - g * e) * inv;
  const r10 = (c * h - b * i) * inv;
  const r11 = (a * i - g * c) * inv;
  const r12 = (g * b - a * h) * inv;
  const r20 = (b * f - c * e) * inv;
  const r21 = (d * c - a * f) * inv;
  const r22 = (a * e - d * b) * inv;
  const tx = m[12];
  const ty = m[13];
  const tz = m[14];
  return [
    r00, r10, r20, 0,
    r01, r11, r21, 0,
    r02, r12, r22, 0,
    -(r00 * tx + r01 * ty + r02 * tz),
    -(r10 * tx + r11 * ty + r12 * tz),
    -(r20 * tx + r21 * ty + r22 * tz),
    1,
  ];
}

export function add(a: Vec3, b: Vec3): Vec3 {
  return [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
}

export function sub(a: Vec3, b: Vec3): Vec3 {
  return [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
}

export function scale(a: Vec3, s: number): Vec3 {
  return [a[0] * s, a[1] * s, a[2] * s];
}

export function length(a: Vec3): number {
  return Math.hypot(a[0], a[1], a[2]);
}

export interface Box3 {
  min: Vec3;
  max: Vec3;
}

export function emptyBox(): Box3 {
  return { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] };
}

export function isEmpty(b: Box3): boolean {
  return !(b.min[0] <= b.max[0] && b.min[1] <= b.max[1] && b.min[2] <= b.max[2]);
}

export function expandPoint(b: Box3, p: Vec3): void {
  for (let k = 0; k < 3; k++) {
    if (p[k] < b.min[k]) b.min[k] = p[k];
    if (p[k] > b.max[k]) b.max[k] = p[k];
  }
}

export function corners(b: Box3): Vec3[] {
  const out: Vec3[] = [];
  for (let i = 0; i < 8; i++) {
    out.push([i & 1 ? b.max[0] : b.min[0], i & 2 ? b.max[1] : b.min[1], i & 4 ? b.max[2] : b.min[2]]);
  }
  return out;
}

/** Bounds of `b` after transforming it with `m`. */
export function transformBox(m: Mat4, b: Box3): Box3 {
  const o = emptyBox();
  for (const c of corners(b)) expandPoint(o, apply(m, c));
  return o;
}

export function containsPoint(b: Box3, p: Vec3, eps: number): boolean {
  for (let k = 0; k < 3; k++) {
    if (p[k] < b.min[k] - eps || p[k] > b.max[k] + eps) return false;
  }
  return true;
}

export function boxDiagonal(b: Box3): number {
  return isEmpty(b) ? 0 : length(sub(b.max, b.min));
}
