// Three.js objects for handles (Plan 9: Gizmos): grips as screen-sized dots, guide lines of
// 1D handles, rotation rings, snap targets during a drag. Colours by handle style: literal,
// module default (different colour), reference only, locked (grey).

import * as THREE from "three";
import { LineSegments2 } from "three/addons/lines/LineSegments2.js";
import { LineSegmentsGeometry } from "three/addons/lines/LineSegmentsGeometry.js";
import type { Vec3 } from "../engine/protocol";
import type { HandleGeom, HandleStyle } from "../logic/handles";
import { add, scale } from "../logic/math";
import type { DragPreview } from "../app/drag";
import { LineMaterial } from "three/addons/lines/LineMaterial.js";

export interface HandleColors {
  literal: string;
  default: string;
  ref: string;
  locked: string;
  snap: string;
}

export function readHandleColors(el: HTMLElement = document.documentElement): HandleColors {
  const cs = getComputedStyle(el);
  const v = (n: string, d: string) => cs.getPropertyValue(n).trim() || d;
  return {
    literal: v("--view-handle", "#ff7a1a"),
    default: v("--view-handle-default", "#b04dff"),
    ref: v("--view-handle-ref", "#0ea5a4"),
    locked: v("--view-handle-locked", "#9aa1ab"),
    snap: v("--view-snap", "#16a34a"),
  };
}

let dotTex: THREE.Texture | null = null;
let ringTex: THREE.Texture | null = null;

function texture(kind: "dot" | "ring"): THREE.Texture {
  if (kind === "dot" && dotTex) return dotTex;
  if (kind === "ring" && ringTex) return ringTex;
  const c = document.createElement("canvas");
  c.width = c.height = 64;
  const g = c.getContext("2d")!;
  if (kind === "dot") {
    g.fillStyle = "#fff";
    g.beginPath();
    g.arc(32, 32, 27, 0, Math.PI * 2);
    g.fill();
    g.strokeStyle = "rgba(0,0,0,0.55)";
    g.lineWidth = 5;
    g.stroke();
  } else {
    g.strokeStyle = "#fff";
    g.lineWidth = 9;
    g.beginPath();
    g.arc(32, 32, 24, 0, Math.PI * 2);
    g.stroke();
  }
  const t = new THREE.CanvasTexture(c);
  if (kind === "dot") dotTex = t;
  else ringTex = t;
  return t;
}

function points(pts: Vec3[], color: string, size: number, kind: "dot" | "ring", order: number, name: string): THREE.Points {
  const geo = new THREE.BufferGeometry();
  geo.setAttribute("position", new THREE.Float32BufferAttribute(pts.flat(), 3));
  const mat = new THREE.PointsMaterial({ color, size, sizeAttenuation: false, map: texture(kind), transparent: true, alphaTest: 0.25, depthTest: false, depthWrite: false });
  const p = new THREE.Points(geo, mat);
  p.renderOrder = order;
  p.userData.ownGeometry = true;
  p.userData.ownMaterial = true;
  p.name = name;
  return p;
}

function styleColor(c: HandleColors, s: HandleStyle): string {
  return c[s];
}

export interface LayerInput {
  geoms: HandleGeom[];
  preview: DragPreview | null;
  hovered: number | null;
  colors: HandleColors;
  resolution: { w: number; h: number };
}

/** Rebuilds the handle layer into `group`. */
export function buildHandleLayer(group: THREE.Group, input: LayerInput): void {
  const { colors, preview } = input;
  const segs = (pos: number[], color: string, width: number, dashed: boolean) => {
    if (pos.length < 6) return;
    const geo = new LineSegmentsGeometry();
    geo.setPositions(pos);
    const mat = new LineMaterial({ color: new THREE.Color(color).getHex(), linewidth: width, depthTest: false, transparent: true, dashed, dashSize: 1, gapSize: 0.6 });
    mat.resolution.set(input.resolution.w, input.resolution.h);
    const l = new LineSegments2(geo, mat);
    l.userData.ownMaterial = true;
    if (dashed) l.computeLineDistances();
    l.renderOrder = 70;
    l.userData.ownGeometry = true;
    group.add(l);
  };
  for (const g of input.geoms) {
    const dragging = preview?.handle === g.handle.id;
    const color = styleColor(colors, g.style);
    const grip = dragging ? preview!.grip : g.grip;
    if (g.line) {
      const b = g.line.base;
      segs([...b, ...grip], color, 1.6, g.style === "locked");
    }
    if (g.ring) {
      const r = g.ring;
      const pos: number[] = [];
      const n = 72;
      for (let i = 0; i < n; i++) {
        const a0 = (i / n) * Math.PI * 2;
        const a1 = ((i + 1) / n) * Math.PI * 2;
        const p0 = add(r.center, add(scale(r.u, Math.cos(a0) * r.radius), scale(r.v, Math.sin(a0) * r.radius)));
        const p1 = add(r.center, add(scale(r.u, Math.cos(a1) * r.radius), scale(r.v, Math.sin(a1) * r.radius)));
        pos.push(...p0, ...p1);
      }
      segs(pos, color, dragging ? 2.6 : 1.8, g.style === "locked");
      segs([...r.center, ...grip], color, 1.2, true);
    }
    const hovered = input.hovered === g.handle.id || dragging;
    const size = g.style === "locked" ? 10 : hovered ? 17 : 13;
    group.add(points([grip], color, size, g.style === "locked" ? "ring" : "dot", 80, `handle:${g.handle.id}`));
  }
  if (preview) {
    if (preview.targets.length) {
      group.add(points(preview.targets.map((t) => t.world), colors.snap, 9, "ring", 75, "snap-targets"));
    }
    if (preview.snap) {
      group.add(points([preview.snap.world], colors.snap, 22, "ring", 85, "snap-active"));
    }
  }
}
