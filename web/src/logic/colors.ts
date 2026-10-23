// Model colours in the view and the colour list (Plan 16): palette display values per theme,
// colours per face (from `Scene.tag_nodes`), rows of "Farben & Filamente".

import type { ColorInfo, MeshData, Rgba, Scene, Vec3 } from "../engine/protocol";
import type { RegionsState, Theme } from "../state/store";
import { apply, invertAffine, mul } from "./math";
import { DEFAULT_COLOR, PALETTE } from "./palette";

/** Density of PLA for the mass estimate (g/cm³), as in the CLI. */
export const PLA_DENSITY = 1.24;
/** Highest filament slot (`color(…, filament = n)`). */
export const MAX_FILAMENT = 16;

export function grams(volumeMm3: number): number {
  return (volumeMm3 * PLA_DENSITY) / 1000;
}

const hex2 = (x: number) =>
  Math.round(Math.min(1, Math.max(0, x)) * 255)
    .toString(16)
    .padStart(2, "0");

/** `#rrggbb` of an RGBA colour (0..1). */
export function rgbaHex(c: Rgba | [number, number, number, number]): string {
  return `#${hex2(c[0])}${hex2(c[1])}${hex2(c[2])}`;
}

const BY_HEX = new Map(PALETTE.map((p) => [p.hex, p]));

/**
 * What the view shows for a model colour: palette colours in their theme variant (black a bit
 * lighter on dark, white a bit darker on light backgrounds); others as they are.
 */
export function displayHex(c: Rgba | [number, number, number, number], theme: Theme): string {
  const h = rgbaHex(c);
  const p = BY_HEX.get(h);
  return p ? (theme === "dark" ? p.dark : p.light) : h;
}

/** Display colour of a colour list entry (the default entry: the theme's model colour). */
export function entryHex(c: ColorInfo, theme: Theme, modelColor: string): string {
  return c.source === "default" ? modelColor : displayHex(c.rgba, theme);
}

/** Face tag → display colour, from the first node with `color` of each tag. */
export function tagColors(scene: Scene, theme: Theme): Map<number, string> {
  const m = new Map<number, string>();
  for (const tn of scene.tag_nodes) {
    for (const n of tn.nodes) {
      const c = scene.nodes[n]?.color;
      if (c) {
        m.set(tn.tag, displayHex(c, theme));
        break;
      }
    }
  }
  return m;
}

/** Colours of the faces of one node's mesh (see {@link faceColors}). */
export interface FaceColors {
  /** Display colour of triangle `t` of `mesh` (undefined: the fallback). */
  colorOf(t: number, mesh: Pick<MeshData, "positions" | "indices" | "tri_tags">): string | undefined;
  /** Changes when a colour of these faces can change (cache key of coloured geometries). */
  sig: string;
  /** Number of face tags with a colour. */
  count: number;
}

/**
 * Colours per face of the mesh of node `owner` (its own system), from `color()` of the node
 * that created the face (Bericht 12, F1). A face tag belongs to every node of equal content
 * (the engine shares their cached result), so only nodes below `owner` count, and of several
 * with different colours the one whose bounds contain the triangle (the latest in pre-order
 * if several do). Approximate where colours overlap or a fillet joins them: the colour
 * regions (drawn as soon as they are computed) are exact.
 */
export function faceColors(scene: Scene, owner: number, theme: Theme): FaceColors {
  const below = (n: number): boolean => {
    for (let i: number | null = n; i !== null && i !== undefined; i = scene.nodes[i]?.parent ?? null) if (i === owner) return true;
    return false;
  };
  const fixed = new Map<number, string>();
  const ambiguous = new Map<number, number[]>();
  const sig: string[] = [theme];
  for (const tn of scene.tag_nodes) {
    const nodes = tn.nodes.filter(below);
    if (!nodes.length) continue;
    const cols = nodes.map((n) => scene.nodes[n]?.color);
    const hex = cols.map((c) => (c ? displayHex(c, theme) : ""));
    if (hex.every((h) => h === hex[0])) {
      if (hex[0]) {
        fixed.set(tn.tag, hex[0]);
        sig.push(`${tn.tag}:${hex[0]}`);
      }
    } else {
      ambiguous.set(tn.tag, nodes);
      sig.push(`${tn.tag}:${nodes.map((n, i) => `${hex[i]}@${scene.nodes[n].display_world.join(",")}`).join(";")}`);
    }
  }
  const ow = scene.nodes[owner]?.display_world;
  const local = new Map<number, ReturnType<typeof invertAffine>>();
  return {
    count: fixed.size + ambiguous.size,
    sig: `${owner}@${ow?.join(",") ?? ""}|${sig.join(",")}`,
    colorOf(t, mesh) {
      const tag = mesh.tri_tags[t];
      const f = fixed.get(tag);
      if (f !== undefined || !ambiguous.has(tag) || !ow) return f;
      const cands = ambiguous.get(tag)!;
      const p: Vec3 = [0, 0, 0];
      for (let k = 0; k < 3; k++) {
        const v = mesh.indices[3 * t + k] * 3;
        for (let a = 0; a < 3; a++) p[a] += mesh.positions[v + a] / 3;
      }
      let pick = -1;
      for (const n of cands) {
        const node = scene.nodes[n];
        const b = node.bounds;
        if (!b) continue;
        let m = local.get(n);
        if (!m) {
          m = mul(invertAffine(node.display_world), ow);
          local.set(n, m);
        }
        const q = apply(m, p);
        const d = Math.hypot(b.max[0] - b.min[0], b.max[1] - b.min[1], b.max[2] - b.min[2]);
        const eps = 1e-6 * d + 1e-4;
        if ([0, 1, 2].every((a) => q[a] >= b.min[a] - eps && q[a] <= b.max[a] + eps)) pick = Math.max(pick, n);
      }
      const c = scene.nodes[pick >= 0 ? pick : cands[0]]?.color;
      return c ? displayHex(c, theme) : undefined;
    },
  };
}

/** Does the scene colour any face? */
export function hasFaceColors(scene: Scene): boolean {
  return (scene.colors ?? []).some((c) => c.source !== "default");
}

function rgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16) / 255) as [number, number, number];
}

/** sRGB → linear (three.js colours are linear in the shader). */
function lin(c: number): number {
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/**
 * Per-vertex colours (linear RGB) of a mesh: every vertex gets the colour of a triangle that
 * uses it (kernel meshes have their own vertices per face, so faces do not bleed).
 */
export function vertexColors(
  mesh: Pick<MeshData, "positions" | "indices" | "tri_tags">,
  colorOf: (tri: number, mesh: Pick<MeshData, "positions" | "indices" | "tri_tags">) => string | undefined,
  fallback: string,
): Float32Array {
  const n = mesh.positions.length / 3;
  const out = new Float32Array(n * 3);
  const base = rgb(fallback).map(lin);
  for (let v = 0; v < n; v++) out.set(base, v * 3);
  const cache = new Map<string, number[]>();
  const tris = mesh.indices.length / 3;
  for (let t = 0; t < tris; t++) {
    const hex = colorOf(t, mesh);
    if (!hex) continue;
    let c = cache.get(hex);
    if (!c) {
      c = rgb(hex).map(lin);
      cache.set(hex, c);
    }
    for (let k = 0; k < 3; k++) out.set(c, mesh.indices[3 * t + k] * 3);
  }
  return out;
}

/** Signature of a tag colour map (cache key of coloured geometries). */
export function colorSignature(m: Map<number, string>): string {
  return [...m].map(([t, c]) => `${t}:${c}`).join(",");
}

export interface ColorRow {
  key: string;
  label: string;
  source: ColorInfo["source"];
  /** Display colour (theme). */
  swatch: string;
  /** Base value (exports). */
  hex: string;
  filament: number;
  explicit: boolean;
  lines: number[];
  /** `color()` statements: line, file (null: main file), offset in the file, label. */
  places: Array<{ line: number; file: string | null; from: number; label: string }>;
  /** mm³ over all parts (null: regions not computed yet). */
  volume: number | null;
  grams: number | null;
}

/** Rows of the colour list; volumes from regions of the same scene version. */
export function colorRows(scene: Scene, regions: RegionsState | null, theme: Theme, modelColor: string): ColorRow[] {
  const reg = regions && regions.version === scene.version ? regions.items : null;
  return (scene.colors ?? []).map((c) => {
    const volume = reg ? reg.flatMap((s) => s.regions).filter((r) => r.color === c.key).reduce((a, r) => a + r.volume, 0) : null;
    return {
      key: c.key,
      label: c.label,
      source: c.source,
      swatch: entryHex(c, theme, modelColor),
      hex: c.source === "default" ? DEFAULT_COLOR : rgbaHex(c.rgba),
      filament: c.filament,
      explicit: c.explicit,
      lines: [...new Set(c.ranges.filter((r) => !r.file).map((r) => r.line))],
      places: c.ranges
        .map((r) => {
          const file = r.file ? (scene.files ?? []).find((f) => f.id === r.file)?.path ?? null : null;
          const short = file ? file.split("/").pop() : null;
          return { line: r.line, file, from: r.from, label: short ? `${short}:${r.line}` : `Z.${r.line}` };
        })
        .filter((p, i, a) => a.findIndex((q) => q.label === p.label) === i),
      volume,
      grams: volume === null ? null : grams(volume),
    };
  });
}

/** Slots used by more than one colour (they print with the same filament). */
export function sharedSlots(rows: ColorRow[]): number[] {
  const n = new Map<number, number>();
  for (const r of rows) n.set(r.filament, (n.get(r.filament) ?? 0) + 1);
  return [...n].filter(([, k]) => k > 1).map(([s]) => s);
}
