// Model colours in the view and the colour list (Plan 16): palette display values per theme,
// colours per face (from `Scene.tag_nodes`), rows of "Farben & Filamente".

import type { ColorInfo, MeshData, Rgba, Scene } from "../engine/protocol";
import type { RegionsState, Theme } from "../state/store";
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
export function vertexColors(mesh: Pick<MeshData, "positions" | "indices" | "tri_tags">, colorOf: (tag: number) => string | undefined, fallback: string): Float32Array {
  const n = mesh.positions.length / 3;
  const out = new Float32Array(n * 3);
  const base = rgb(fallback).map(lin);
  for (let v = 0; v < n; v++) out.set(base, v * 3);
  const cache = new Map<string, number[]>();
  const tris = mesh.indices.length / 3;
  for (let t = 0; t < tris; t++) {
    const hex = colorOf(mesh.tri_tags[t]);
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
      lines: [...new Set(c.ranges.map((r) => r.line))],
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
