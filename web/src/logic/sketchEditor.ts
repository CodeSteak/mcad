// Sketch editor (Plan 17.12), the UI's part: the engine snaps, previews and writes the code;
// here are only the pure pieces around it – the step raster of the drawing plane, the camera
// looking perpendicularly onto the plane, the marker of a snap kind, applying an answer's
// edits as one undo step with the cursor after the new code, and the tool list.

import { isolateHistory } from "@codemirror/commands";
import type { EditorState, TransactionSpec } from "@codemirror/state";
import type { Edit, Mat4, SketchTool, Vec3 } from "../engine/protocol";
import { mapEdits } from "../editor/applyEdits";
import { lazyRecord, t } from "../i18n";
import { add, apply, applyDir, length, scale, sub } from "./math";
import type { VersionMapper } from "./positions";

/** Tools in the bar (order, label, tooltip). */
export const SKETCH_TOOLS: Array<{ tool: SketchTool; label: string; title: string }> = (["select", "line", "arc_through", "arc_center", "arc_tangent", "bezier", "rect", "circle", "corner"] as const).map((tool) => ({
  tool,
  get label() {
    return t(`sketch.tool.${tool}.short`);
  },
  get title() {
    return t(`sketch.tool.${tool}.title`);
  },
}));

/** Shortcuts of the tools (docs/konzepte/ui-ueberarbeitung, Abschnitt 5). */
export const SKETCH_TOOL_KEYS: Partial<Record<SketchTool, string>> = {
  select: "V",
  line: "L",
  arc_through: "A",
  arc_center: "⇧+A",
  arc_tangent: "T",
  bezier: "B",
  rect: "R",
  circle: "C",
  corner: "F",
};

/** Action of a key in the drawing mode (without Strg/Alt): a tool or a command. */
export function sketchKeyAction(key: string, shift: boolean): { tool: SketchTool } | { command: "close" | "align" } | null {
  if (key === "Enter") return { command: "close" };
  const k = key.length === 1 ? key.toUpperCase() : "";
  if (k === "N" && !shift) return { command: "align" };
  if (k === "A") return { tool: shift ? "arc_center" : "arc_through" };
  if (shift) return null;
  for (const [tool, key2] of Object.entries(SKETCH_TOOL_KEYS)) if (key2 === k) return { tool: tool as SketchTool };
  return null;
}

const ALL_TOOLS = ["select", "line", "arc_through", "arc_center", "arc_tangent", "bezier", "rect", "circle", "corner", "close", "delete"] as const satisfies readonly SketchTool[];

export const SKETCH_TOOL_NAMES: Record<SketchTool, string> = lazyRecord(ALL_TOOLS, (tool) => t(`sketch.tool.${tool}`));

/**
 * Spacing of the step raster: the step times 1, 2, 5, 10, … so that lines are at least
 * `minPx` pixels apart (`worldPerPx`: mm per pixel at the plane).
 */
export function rasterSpacing(step: number, worldPerPx: number, minPx = 12): number {
  let s = step > 0 ? step : 1;
  const f = [2, 2.5, 2];
  let i = 0;
  while (s / Math.max(worldPerPx, 1e-12) < minPx && i < 60) {
    s *= f[i % 3];
    i++;
  }
  return s;
}

/** Lines of the step raster in the plane (local 2D), `major` every fifth. At most `max` per axis. */
export function rasterLines(min: [number, number], max: [number, number], spacing: number, max_ = 160): Array<{ a: [number, number]; b: [number, number]; major: boolean }> {
  const out: Array<{ a: [number, number]; b: [number, number]; major: boolean }> = [];
  if (!(spacing > 0)) return out;
  for (const axis of [0, 1] as const) {
    const lo = Math.ceil(min[axis] / spacing);
    const hi = Math.floor(max[axis] / spacing);
    if (hi - lo > max_) continue;
    for (let k = lo; k <= hi; k++) {
      const v = k * spacing + 0;
      const a: [number, number] = axis === 0 ? [v, min[1]] : [min[0], v];
      const b: [number, number] = axis === 0 ? [v, max[1]] : [max[0], v];
      out.push({ a, b, major: k % 5 === 0 });
    }
  }
  return out;
}

/**
 * Camera looking perpendicularly onto a sketch plane at the extent `min`..`max` (plane
 * coordinates): from the side the plane's z axis points to, the plane's y axis up on the
 * screen. The camera's `up` stays z (orbit controls): looking along z, the eye is tilted by
 * 10⁻⁴ towards −y of the plane, so the plane's y is up as in the top view.
 */
export function planeCamera(plane: Mat4, min: [number, number], max: [number, number], fovDeg: number, aspect: number): { target: Vec3; eye: Vec3; up: Vec3 } {
  const c = apply(plane, [(min[0] + max[0]) / 2, (min[1] + max[1]) / 2, 0]);
  const nz = applyDir(plane, [0, 0, 1]);
  const ny = applyDir(plane, [0, 1, 0]);
  const n = scale(nz, 1 / Math.max(length(nz), 1e-12));
  const y = scale(ny, 1 / Math.max(length(ny), 1e-12));
  const radius = Math.max(1, length(sub(apply(plane, [max[0], max[1], 0]), apply(plane, [min[0], min[1], 0]))) / 2);
  const fov = (fovDeg * Math.PI) / 180;
  const fitH = radius / Math.sin(fov / 2);
  const fitW = radius / Math.sin(Math.atan(Math.tan(fov / 2) * aspect));
  const dist = Math.max(fitH, fitW) * 1.05;
  const alongZ = Math.abs(n[2]) > 0.999;
  const dir = alongZ ? sub(n, scale(y, 1e-4)) : n;
  const d = scale(dir, 1 / length(dir));
  return { target: c, eye: add(c, scale(d, dist)), up: [0, 0, 1] };
}

/** Marker of a snap kind around a point (plane coordinates, radius `r`): ring for points of
 *  the model, square for grid points and anchors, diamond for directions, cross for the step. */
export function snapMarker(kind: string, p: [number, number], r: number): Array<[number, number]> {
  const [x, y] = p;
  switch (kind) {
    case "grid":
    case "anchor":
      return [[x - r, y - r], [x + r, y - r], [x + r, y + r], [x - r, y + r], [x - r, y - r]];
    case "tangent":
    case "horizontal":
    case "vertical":
    case "grid_line":
      return [[x - r, y], [x, y - r], [x + r, y], [x, y + r], [x - r, y]];
    case "step":
      return [[x - r, y - r], [x + r, y + r], [x, y], [x - r, y + r], [x + r, y - r]];
    default: {
      const pts: Array<[number, number]> = [];
      for (let i = 0; i <= 20; i++) {
        const a = (i / 20) * Math.PI * 2;
        pts.push([x + r * Math.cos(a), y + r * Math.sin(a)]);
      }
      return pts;
    }
  }
}

/**
 * Transaction of an answer's edits: one history event of its own (Undo takes the whole action
 * back), the cursor right after the new code (`select`: edit index and offset in its text), so
 * the next action goes after it. `null` if the version is unknown or the edits overlap.
 */
export function sketchTransaction(state: EditorState, mapper: VersionMapper, version: number, edits: Edit[], select: [number, number] | null): TransactionSpec | null {
  const changes = mapEdits(mapper, version, edits, state.doc.length);
  if (!changes) return null;
  const cs = state.changes(changes);
  let anchor: number | undefined;
  if (select && edits[select[0]]) {
    const m = mapper.mapper(version);
    const at = Math.min(m(edits[select[0]].from, -1), state.doc.length);
    anchor = Math.min(cs.mapPos(at, -1) + select[1], cs.newLength);
  }
  return {
    changes: cs,
    selection: anchor !== undefined ? { anchor } : undefined,
    scrollIntoView: true,
    userEvent: "input.sketch",
    annotations: isolateHistory.of("full"),
  };
}

/** Pixel size at distance 1 of a perspective camera (vertical field of view, viewport height). */
export function pixelSize(fovDeg: number, heightPx: number): number {
  return (2 * Math.tan((fovDeg * Math.PI) / 360)) / Math.max(1, heightPx);
}
