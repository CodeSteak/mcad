// Studio settings ↔ the file's `render { … }` block: the settings a block gives, and the
// changes (`SetRender`) that write a settings change into the block. Pure functions (unit tests).

import type { RenderBlock, RenderChange, RenderTarget, Value } from "../engine/protocol";
import { DEFAULT_STUDIO, ENVIRONMENTS, type EnvironmentName, KEY_LIGHT, type MaterialTarget, type StudioLight, type StudioMaterial, type StudioSettings } from "./studio";

const r3 = (v: number) => Math.round(v * 1000) / 1000;

/** The studio settings of a render block (unset values: the defaults); `on` stays. */
export function settingsFromBlock(b: RenderBlock, on: boolean): StudioSettings {
  const d = DEFAULT_STUDIO;
  const lights: StudioLight[] = b.lights.length ? b.lights.map((l) => ({ name: l.name, azimuth: l.azimuth, elevation: l.elevation, intensity: l.intensity, color: l.color.toLowerCase() })) : [KEY_LIGHT];
  const bg = b.background ?? null;
  return {
    on,
    lights,
    lightIntensity: b.light_intensity ?? d.lightIntensity,
    ambient: b.ambient ?? d.ambient,
    environment: ENVIRONMENTS.includes(b.environment as EnvironmentName) ? (b.environment as EnvironmentName) : d.environment,
    shadows: b.shadows ?? d.shadows,
    shadowSoftness: b.shadow_softness ?? d.shadowSoftness,
    ao: b.ao ?? d.ao,
    aoRadius: b.ao_radius ?? null,
    aoStrength: b.ao_strength ?? d.aoStrength,
    ground: b.ground ?? d.ground,
    groundColor: b.ground_color?.toLowerCase() ?? null,
    groundReflection: b.ground_reflection ?? d.groundReflection,
    groundRoughness: b.ground_roughness ?? d.groundRoughness,
    reflections: b.reflections ?? d.reflections,
    background: bg && bg.length >= 1 ? [bg[0].toLowerCase(), (bg[1] ?? bg[0]).toLowerCase()] : null,
    edges: b.edges ?? d.edges,
    materials: b.materials.map((m) => ({ target: "part" in m.target ? { part: m.target.part } : { color: m.target.color }, preset: m.preset, metalness: m.metalness, roughness: m.roughness, clearcoat: m.clearcoat, anisotropy: m.anisotropy })),
  };
}

/** A key that changes when the block's values change (not when only its place moves). */
export function blockKey(b: RenderBlock | null | undefined): string | null {
  if (!b) return null;
  const { range: _r, ...rest } = b;
  return JSON.stringify({ ...rest, lights: rest.lights.map(({ range: _x, ...l }) => l), materials: rest.materials.map(({ range: _x, ...m }) => m) });
}

type Num = number | null;

/** Top-level settings: field of the settings, key in the block, value text. */
const TOP: Array<[keyof StudioSettings, string, (s: StudioSettings) => Value]> = [
  ["edges", "edges", (s) => s.edges],
  ["environment", "environment", (s) => s.environment],
  ["ambient", "ambient", (s) => r3(s.ambient)],
  ["lightIntensity", "light_intensity", (s) => r3(s.lightIntensity)],
  ["shadows", "shadows", (s) => s.shadows],
  ["shadowSoftness", "shadow_softness", (s) => r3(s.shadowSoftness)],
  ["ao", "ao", (s) => s.ao],
  ["aoRadius", "ao_radius", (s) => (s.aoRadius === null ? null : r3(s.aoRadius))],
  ["aoStrength", "ao_strength", (s) => r3(s.aoStrength)],
  ["ground", "ground", (s) => s.ground],
  ["groundColor", "ground_color", (s) => s.groundColor],
  ["groundReflection", "ground_reflection", (s) => r3(s.groundReflection)],
  ["groundRoughness", "ground_roughness", (s) => r3(s.groundRoughness)],
  ["reflections", "reflections", (s) => s.reflections],
  ["background", "background", (s) => (s.background === null ? null : s.background[0] === s.background[1] ? s.background[0] : [s.background[0], s.background[1]])],
];

const LIGHT_KEYS: Array<[keyof StudioLight, string]> = [
  ["azimuth", "azimuth"],
  ["elevation", "elevation"],
  ["intensity", "intensity"],
  ["color", "color"],
];
const MATERIAL_KEYS: Array<keyof Omit<StudioMaterial, "target">> = ["preset", "metalness", "roughness", "clearcoat", "anisotropy"];

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
const lightValue = (l: StudioLight, k: keyof StudioLight): Value => (typeof l[k] === "number" ? r3(l[k] as number) : (l[k] as string));
const targetOf = (t: MaterialTarget): RenderTarget => ("part" in t ? { part: t.part } : { color: t.color });
const sameTarget = (a: MaterialTarget, b: MaterialTarget) => ("part" in a ? "part" in b && a.part === b.part : "color" in b && a.color.toLowerCase() === b.color.toLowerCase());
const matValue = (m: StudioMaterial, k: (typeof MATERIAL_KEYS)[number]): Value => {
  const v = m[k] as string | Num;
  return typeof v === "number" ? r3(v) : v;
};

/**
 * Changes of the render block from `prev` to `next`. Without a block (`hasBlock` false) the new
 * block gets every value that differs from the defaults, so the file shows what the view shows.
 */
export function blockChanges(prev: StudioSettings, next: StudioSettings, hasBlock: boolean): RenderChange[] {
  const base = hasBlock ? prev : { ...DEFAULT_STUDIO, on: next.on };
  const out: RenderChange[] = [];
  for (const [field, key, val] of TOP) {
    if (!same(base[field], next[field])) out.push({ kind: "set", key, value: val(next) });
  }
  // lights by name; the default single key light needs no entry in a new block
  // the default key light may be implicit (a block without lights): once the lights differ
  // from it, all of them are written
  const implicit = same(base.lights, [KEY_LIGHT]);
  const baseLights = hasBlock && !implicit ? base.lights : [];
  const unchanged = hasBlock ? same(base.lights, next.lights) : same(next.lights, [KEY_LIGHT]);
  const nextLights = unchanged ? baseLights : next.lights;
  for (const l of baseLights) if (!nextLights.some((x) => x.name === l.name)) out.push({ kind: "remove_light", name: l.name });
  for (const l of nextLights) {
    const old = baseLights.find((x) => x.name === l.name);
    for (const [f, key] of LIGHT_KEYS) {
      // a new entry: its direction and what differs from the key light's values
      const changed = old ? old[f] !== l[f] : f === "azimuth" || f === "elevation" || l[f] !== KEY_LIGHT[f];
      if (changed) out.push({ kind: "light", name: l.name, key, value: lightValue(l, f) });
    }
  }
  // materials by target
  for (const m of base.materials) if (!next.materials.some((x) => sameTarget(x.target, m.target))) out.push({ kind: "remove_material", target: targetOf(m.target) });
  for (const m of next.materials) {
    const old = base.materials.find((x) => sameTarget(x.target, m.target));
    for (const k of MATERIAL_KEYS) {
      if (old ? old[k] !== m[k] : m[k] !== null) out.push({ kind: "material", target: targetOf(m.target), key: k, value: matValue(m, k) });
    }
  }
  return out;
}
