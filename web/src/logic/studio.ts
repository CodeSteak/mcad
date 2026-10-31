// Studio look of the 3D view (like `mcad render --style studio`): settings, defaults, material
// presets, the analytic environment (the same numbers as `cad_render::studio`), the light
// direction relative to the camera and the equivalent CLI call. Pure functions (unit tests).

import type { Vec3 } from "../engine/protocol";
import { cliCamera } from "./section";

/** A directional light; azimuth relative to the camera, elevation above the xy plane. */
export interface StudioLight {
  /** Name in the `render` block (`light key { … }`). */
  name: string;
  /** Degrees; 0 = from the camera, + = from the right. */
  azimuth: number;
  /** Degrees above the xy plane. */
  elevation: number;
  /** 0…4 (1 = default key light). */
  intensity: number;
  /** `#rrggbb`. */
  color: string;
}

/** Which bodies a material entry applies to: a colour (`red`, `#rrggbb`, `default` =
 *  uncoloured) or a part. */
export type MaterialTarget = { color: string } | { part: string };

/** A material entry: preset and single values (`null` = from the preset). */
export interface StudioMaterial {
  target: MaterialTarget;
  preset: string | null;
  metalness: number | null;
  roughness: number | null;
  clearcoat: number | null;
  anisotropy: number | null;
}

/** Physically based surface (metallic-roughness). */
export interface Pbr {
  metalness: number;
  roughness: number;
  clearcoat: number;
  /** Brushed finish 0…1 (anisotropic GGX, brushing around the body's z axis). */
  anisotropy: number;
  /** Own base colour (linear RGB, e.g. the reflectance of gold); `null`: the body's colour. */
  color: Vec3 | null;
}

export type EnvironmentName = "studio" | "soft" | "overcast" | "none";
export const ENVIRONMENTS: EnvironmentName[] = ["studio", "soft", "overcast", "none"];

export interface StudioSettings {
  /** Studio look on. */
  on: boolean;
  /** Directional lights; the first casts the shadows. */
  lights: StudioLight[];
  /** Factor on all lights, 0…4. */
  lightIntensity: number;
  /** Share of the environment light, 0…1. */
  ambient: number;
  /** Surroundings seen in reflections. */
  environment: EnvironmentName;
  shadows: boolean;
  /** 0 = hard … 1 = very soft. */
  shadowSoftness: number;
  ao: boolean;
  /** Ambient occlusion radius in mm; `null` = 5 % of the model diagonal. */
  aoRadius: number | null;
  /** 0…2. */
  aoStrength: number;
  ground: boolean;
  /** `#rrggbb`; `null` = from the theme. */
  groundColor: string | null;
  /** Mirror image of the model in the ground, 0…1. */
  groundReflection: number;
  /** Blur of the ground reflection, 0 = mirror … 1. */
  groundRoughness: number;
  /** Ray-traced reflections of the model in itself (`mcad render`; the view shows the
   *  environment and the ground reflection). */
  reflections: boolean;
  /** Background gradient top, bottom (`#rrggbb`, equal = solid); `null` = from the theme. */
  background: [string, string] | null;
  /** Edge lines of the bodies. */
  edges: boolean;
  /** Materials by colour or part (later entries win). */
  materials: StudioMaterial[];
}

export const KEY_LIGHT: StudioLight = { name: "key", azimuth: -40, elevation: 50, intensity: 1, color: "#ffffff" };

/** Defaults: the same as `mcad render --style studio`. */
export const DEFAULT_STUDIO: StudioSettings = {
  on: false,
  lights: [KEY_LIGHT],
  lightIntensity: 1,
  ambient: 0.4,
  environment: "studio",
  shadows: true,
  shadowSoftness: 0.35,
  ao: true,
  aoRadius: null,
  aoStrength: 1,
  ground: true,
  groundColor: null,
  groundReflection: 0.2,
  groundRoughness: 0.3,
  reflections: true,
  background: null,
  edges: true,
  materials: [],
};

/** Theme colours of the studio look (as the CLI's `--theme light|dark`). */
export const STUDIO_THEME = {
  light: { ground: "#e6e8ec", background: ["#f8f9fb", "#d9dde3"] as [string, string] },
  dark: { ground: "#2c3038", background: ["#2b3038", "#111317"] as [string, string] },
};

/** Reflectance (F0, linear RGB) of real metals (as `cad_render::studio::f0`). */
export const F0 = {
  aluminium: [0.913, 0.922, 0.924] as Vec3,
  chrome: [0.55, 0.556, 0.554] as Vec3,
  stainless: [0.65, 0.64, 0.62] as Vec3,
  titanium: [0.542, 0.497, 0.449] as Vec3,
  gold: [1.0, 0.766, 0.336] as Vec3,
  copper: [0.955, 0.638, 0.538] as Vec3,
  brass: [0.91, 0.778, 0.423] as Vec3,
};

const mat = (metalness: number, roughness: number, clearcoat = 0): Pbr => ({ metalness, roughness, clearcoat, anisotropy: 0, color: null });
const metal = (color: Vec3, roughness: number, anisotropy = 0): Pbr => ({ metalness: 1, roughness, clearcoat: 0, anisotropy, color });

/** Material presets (as `cad_render::Material::PRESETS`, in that order). */
export const MATERIAL_PRESETS: Record<string, Pbr> = {
  plastic: mat(0, 0.45),
  plastic_satin: mat(0, 0.3),
  plastic_glossy: mat(0, 0.12, 0.6),
  rubber: mat(0, 0.85),
  ceramic: mat(0, 0.35, 1),
  aluminium: metal(F0.aluminium, 0.2),
  metal_brushed: metal(F0.aluminium, 0.22, 0.8),
  anodized: mat(1, 0.28),
  chrome: metal(F0.chrome, 0.04),
  stainless: metal(F0.stainless, 0.12),
  titanium: metal(F0.titanium, 0.25),
  gold: metal(F0.gold, 0.12),
  copper: metal(F0.copper, 0.18),
  brass: metal(F0.brass, 0.15),
};
export const PRESET_NAMES = Object.keys(MATERIAL_PRESETS);
export const DEFAULT_PBR: Pbr = MATERIAL_PRESETS.plastic;

/** Widening of the specular lobe of the lights (round area lights of about 10°), as
 *  `cad_render::studio::LIGHT_SPREAD`. */
export const LIGHT_SPREAD = 0.088;

const HEX = /^#[0-9a-f]{6}$/;
const clamp01 = (v: number) => Math.max(0, Math.min(1, v));

/** The surface of a material entry: the preset (else plastic) with the single values over it. */
export function resolvePbr(m: Pick<StudioMaterial, "preset" | "metalness" | "roughness" | "clearcoat" | "anisotropy">): Pbr {
  const b = (m.preset && MATERIAL_PRESETS[m.preset]) || DEFAULT_PBR;
  return {
    metalness: clamp01(m.metalness ?? b.metalness),
    roughness: clamp01(m.roughness ?? b.roughness),
    clearcoat: clamp01(m.clearcoat ?? b.clearcoat),
    anisotropy: clamp01(m.anisotropy ?? b.anisotropy),
    color: b.color,
  };
}

/** A colour of the model as the material lookup sees it. */
export interface ColorRef {
  /** `#rrggbb` (or `#rrggbbaa`). */
  key: string;
  /** Palette name, unknown name, hex text or `default`. */
  label: string;
  /** Uncoloured material. */
  isDefault: boolean;
}

const sameTarget = (a: MaterialTarget, b: MaterialTarget) =>
  "part" in a ? "part" in b && a.part === b.part : "color" in b && a.color.toLowerCase() === b.color.toLowerCase();

/** Does a colour entry name this colour (palette name, hex, `default`)? */
export function colorMatches(sel: string, c: ColorRef | null): boolean {
  const s = sel.trim().toLowerCase();
  if (s === "default") return c === null || c.isDefault;
  if (!c || c.isDefault) return false;
  return s === c.label.toLowerCase() || s === c.key.slice(0, 7).toLowerCase();
}

/** Surface of a body of colour `c` (`null`: uncoloured) in part `part`: the last part entry,
 *  else the last colour entry, else plastic (as the CLI). */
export function materialFor(materials: StudioMaterial[], c: ColorRef | null, part: string | null): Pbr {
  for (let i = materials.length - 1; i >= 0; i--) {
    const m = materials[i];
    // `feet` names all parts `feet[k]`
    if ("part" in m.target && part !== null && (m.target.part === part || part.startsWith(`${m.target.part}[`))) return resolvePbr(m);
  }
  for (let i = materials.length - 1; i >= 0; i--) {
    const m = materials[i];
    if ("color" in m.target && colorMatches(m.target.color, c)) return resolvePbr(m);
  }
  return DEFAULT_PBR;
}

/** The studio look has materials of parts: the view draws the bodies per part (colour regions). */
export function studioPartMaterials(s: StudioSettings): boolean {
  return s.on && s.materials.some((m) => "part" in m.target);
}

/** Materials with the entry for `target` replaced (`null`: removed). */
export function withMaterial(materials: StudioMaterial[], target: MaterialTarget, m: Omit<StudioMaterial, "target"> | null): StudioMaterial[] {
  const i = materials.findIndex((x) => sameTarget(x.target, target));
  if (m === null) return i < 0 ? materials : materials.filter((_, k) => k !== i);
  const entry = { target, ...m };
  return i < 0 ? [...materials, entry] : materials.map((x, k) => (k === i ? entry : x));
}

/** Entry of `target` (`null`: none). */
export function materialEntry(materials: StudioMaterial[], target: MaterialTarget): StudioMaterial | null {
  return materials.find((x) => sameTarget(x.target, target)) ?? null;
}

/** A new light name: `light2`, `light3`, … not used yet. */
export function newLightName(lights: StudioLight[]): string {
  for (let k = lights.length + 1; ; k++) {
    const n = `light${k}`;
    if (!lights.some((l) => l.name === n)) return n;
  }
}

/** Settings from stored JSON: unknown or out-of-range values fall back to the defaults. */
export function sanitizeStudio(raw: unknown): StudioSettings {
  const d = DEFAULT_STUDIO;
  if (!raw || typeof raw !== "object") return { ...d };
  const r = raw as Record<string, unknown>;
  const numIn = (v: unknown, lo: number, hi: number, dv: number): number => (typeof v === "number" && Number.isFinite(v) && v >= lo && v <= hi ? v : dv);
  const num = (k: keyof StudioSettings, lo: number, hi: number): number => numIn(r[k], lo, hi, d[k] as number);
  const bool = (k: keyof StudioSettings): boolean => (typeof r[k] === "boolean" ? (r[k] as boolean) : (d[k] as boolean));
  const hex = (v: unknown): string | null => (typeof v === "string" && HEX.test(v.toLowerCase()) ? v.toLowerCase() : null);
  const bg = Array.isArray(r.background) && r.background.length === 2 ? [hex(r.background[0]), hex(r.background[1])] : null;
  let lights: StudioLight[];
  if (Array.isArray(r.lights)) {
    lights = [];
    for (const x of r.lights as unknown[]) {
      if (!x || typeof x !== "object") continue;
      const l = x as Record<string, unknown>;
      const name = typeof l.name === "string" && /^[A-Za-z_][A-Za-z0-9_]*$/.test(l.name) && !lights.some((o) => o.name === l.name) ? l.name : newLightName(lights);
      lights.push({ name, azimuth: numIn(l.azimuth, -360, 360, KEY_LIGHT.azimuth), elevation: numIn(l.elevation, 1, 90, KEY_LIGHT.elevation), intensity: numIn(l.intensity, 0, 4, 1), color: hex(l.color) ?? "#ffffff" });
    }
  } else {
    // settings of the first studio version: one key light
    lights = [{ ...KEY_LIGHT, azimuth: numIn(r.lightAzimuth, -360, 360, KEY_LIGHT.azimuth), elevation: numIn(r.lightElevation, 1, 90, KEY_LIGHT.elevation) }];
  }
  const materials: StudioMaterial[] = [];
  if (Array.isArray(r.materials)) {
    for (const x of r.materials as unknown[]) {
      if (!x || typeof x !== "object") continue;
      const m = x as Record<string, unknown>;
      const t = m.target as Record<string, unknown> | undefined;
      const target: MaterialTarget | null = t && typeof t.part === "string" ? { part: t.part } : t && typeof t.color === "string" ? { color: t.color } : null;
      if (!target) continue;
      const v = (k: string) => (typeof m[k] === "number" && Number.isFinite(m[k] as number) ? clamp01(m[k] as number) : null);
      const preset = typeof m.preset === "string" && m.preset in MATERIAL_PRESETS ? m.preset : null;
      materials.push({ target, preset, metalness: v("metalness"), roughness: v("roughness"), clearcoat: v("clearcoat"), anisotropy: v("anisotropy") });
    }
  }
  return {
    on: bool("on"),
    lights,
    lightIntensity: num("lightIntensity", 0, 4),
    ambient: num("ambient", 0, 1),
    environment: ENVIRONMENTS.includes(r.environment as EnvironmentName) ? (r.environment as EnvironmentName) : d.environment,
    shadows: bool("shadows"),
    shadowSoftness: num("shadowSoftness", 0, 1),
    ao: bool("ao"),
    aoRadius: typeof r.aoRadius === "number" && Number.isFinite(r.aoRadius) && r.aoRadius > 0 ? r.aoRadius : null,
    aoStrength: num("aoStrength", 0, 2),
    ground: bool("ground"),
    groundColor: hex(r.groundColor),
    groundReflection: num("groundReflection", 0, 1),
    groundRoughness: num("groundRoughness", 0, 1),
    reflections: bool("reflections"),
    background: bg && bg[0] && bg[1] ? [bg[0], bg[1]] : null,
    edges: bool("edges"),
    materials,
  };
}

/** Ambient occlusion radius in mm for a model diagonal (`aoRadius` or 5 %). */
export function aoRadiusFor(s: StudioSettings, diagonal: number): number {
  return s.aoRadius ?? Math.max(1e-3, diagonal) * 0.05;
}

/**
 * Unit vector towards a light at `az` degrees relative to the camera azimuth and `el` degrees
 * above the xy plane; `eye` points from the target to the camera, `up` is the screen's up
 * (used when looking straight down or up). Same convention as `cad_render::studio`.
 */
export function lightDirection(eye: Vec3, up: Vec3, az: number, el: number): Vec3 {
  const a = cameraAzimuth(eye, up) + (az * Math.PI) / 180;
  const elc = Math.max(-89.9, Math.min(89.9, el));
  const e = (elc * Math.PI) / 180;
  return [Math.sin(a) * Math.cos(e), -Math.cos(a) * Math.cos(e), Math.sin(e)];
}

/** Azimuth (radians) of the camera around z: 0 = from −y, π/2 = from +x. */
export function cameraAzimuth(eye: Vec3, up: Vec3): number {
  if (Math.abs(eye[0]) < 1e-9 && Math.abs(eye[1]) < 1e-9) {
    const s = Math.sign(eye[2]) || 1;
    const e = [-up[0] * s, -up[1] * s];
    return Math.atan2(e[0], -e[1]);
  }
  return Math.atan2(eye[0], -eye[1]);
}

// ---------------------------------------------------------------------------------------------
// Procedural environment (cad_render::env): a vertical gradient and rectangular softboxes

/** A rectangular softbox facing the model. */
export interface Softbox {
  /** Degrees relative to the camera (as lights). */
  azimuth: number;
  elevation: number;
  /** Angular size in degrees (horizontal, vertical). */
  width: number;
  height: number;
  /** Width of the soft edge in degrees. */
  softness: number;
  /** Linear RGB radiance at the centre. */
  radiance: Vec3;
}
export interface EnvSpec {
  zenith: Vec3;
  horizon: Vec3;
  floorHorizon: Vec3;
  nadir: Vec3;
  /** Half width of the floor → wall transition in degrees. */
  horizonBlur: number;
  boxes: Softbox[];
}

/** Factor of the environment reflections per unit of `ambient`. */
export const ENV_SPECULAR_GAIN = 2.5;

export function srgbToLinear(c: number): number {
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** The environment (`ground`: linear colour of the ground), as `cad_render::env::environment_spec`. */
export function environmentSpec(env: EnvironmentName, ground: Vec3): EnvSpec {
  const g = (k: number): Vec3 => [ground[0] * k, ground[1] * k, ground[2] * k];
  const box = (azimuth: number, elevation: number, width: number, height: number, softness: number, radiance: Vec3): Softbox => ({ azimuth, elevation, width, height, softness, radiance });
  switch (env) {
    case "studio":
      return {
        zenith: [0.05, 0.05, 0.055],
        horizon: [0.012, 0.012, 0.014],
        floorHorizon: g(0.05),
        nadir: g(0.75),
        horizonBlur: 3,
        boxes: [
          box(180, 55, 64, 30, 5, [6, 6, 6]),
          box(-68, 5, 16, 100, 3, [9, 8.6, 8]),
          box(105, 5, 12, 96, 3, [3, 3.15, 3.4]),
          box(0, 8, 110, 22, 14, [0.45, 0.45, 0.47]),
          box(95, -22, 60, 36, 18, [0.3, 0.3, 0.3]),
        ],
      };
    case "soft":
      return {
        zenith: [0.55, 0.56, 0.58],
        horizon: [0.3, 0.3, 0.31],
        floorHorizon: g(0.35),
        nadir: g(0.75),
        horizonBlur: 10,
        boxes: [box(180, 55, 110, 60, 18, [2.2, 2.2, 2.2]), box(-70, 22, 34, 60, 12, [2.6, 2.55, 2.45]), box(105, 22, 30, 56, 12, [1.6, 1.65, 1.7])],
      };
    case "overcast":
      return { zenith: [1, 1.01, 1.05], horizon: [0.45, 0.45, 0.46], floorHorizon: g(0.35), nadir: g(0.6), horizonBlur: 12, boxes: [] };
    case "none":
      return { zenith: [0, 0, 0], horizon: [0, 0, 0], floorHorizon: [0, 0, 0], nadir: [0, 0, 0], horizonBlur: 1, boxes: [] };
  }
}

const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: Vec3, b: Vec3): Vec3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const normalize = (a: Vec3): Vec3 => {
  const l = Math.hypot(a[0], a[1], a[2]);
  return [a[0] / l, a[1] / l, a[2] / l];
};
const smoothstep = (e0: number, e1: number, x: number) => {
  const t = Math.max(0, Math.min(1, (x - e0) / (e1 - e0)));
  return t * t * (3 - 2 * t);
};

interface PlacedBox {
  c: Vec3;
  u: Vec3;
  v: Vec3;
  b: Softbox;
}

/** Softbox frames for a camera at azimuth 0 (looking along +y). */
function placeBoxes(spec: EnvSpec): PlacedBox[] {
  return spec.boxes.map((b) => {
    const c = lightDirection([0, -1, 0], [0, 0, 1], b.azimuth, b.elevation);
    const a = (b.azimuth * Math.PI) / 180;
    const u: Vec3 = Math.abs(c[2]) < 0.999 ? normalize(cross([0, 0, 1], c)) : [Math.cos(a), Math.sin(a), 0];
    return { c, u, v: cross(c, u), b };
  });
}

function radianceOf(spec: EnvSpec, boxes: PlacedBox[], d: Vec3): Vec3 {
  const el = (Math.asin(Math.max(-1, Math.min(1, d[2]))) * 180) / Math.PI;
  const up = Math.max(0, d[2]);
  const down = Math.min(0, d[2]) ** 2;
  const k = smoothstep(-spec.horizonBlur, spec.horizonBlur, el);
  const c = [0, 1, 2].map((i) => {
    const wall = spec.horizon[i] + (spec.zenith[i] - spec.horizon[i]) * up;
    const floor = spec.floorHorizon[i] + (spec.nadir[i] - spec.floorHorizon[i]) * down;
    return floor + (wall - floor) * k;
  }) as Vec3;
  for (const p of boxes) {
    const z = dot(d, p.c);
    if (z <= 1e-3) continue;
    const x = (Math.atan2(dot(d, p.u), z) * 180) / Math.PI;
    const y = (Math.atan2(dot(d, p.v), z) * 180) / Math.PI;
    const hw = p.b.width / 2;
    const hh = p.b.height / 2;
    const sf = Math.max(0.01, p.b.softness);
    const m = (1 - smoothstep(hw - sf, hw + sf, Math.abs(x))) * (1 - smoothstep(hh - sf, hh + sf, Math.abs(y)));
    if (m <= 0) continue;
    const hot = 1 - 0.2 * Math.min(1, (x / hw) ** 2 + (y / hh) ** 2);
    for (let i = 0; i < 3; i++) c[i] += p.b.radiance[i] * m * hot;
  }
  return c;
}

/** Radiance of the environment in direction `d` (unit, z up; camera at azimuth 0). */
export function envRadiance(spec: EnvSpec, d: Vec3): Vec3 {
  return radianceOf(spec, placeBoxes(spec), d);
}

/** Irradiance on a surface facing up divided by π (luminance), as
 *  `cad_render::env::up_irradiance`: midpoint rule over 256 × 64 directions. */
export function upIrradiance(spec: EnvSpec): number {
  const boxes = placeBoxes(spec);
  const na = 256;
  const ne = 64;
  const da = (2 * Math.PI) / na;
  const de = Math.PI / 2 / ne;
  const e = [0, 0, 0];
  for (let j = 0; j < ne; j++) {
    const el = (j + 0.5) * de;
    const w = Math.sin(el) * Math.cos(el) * de * da;
    for (let i = 0; i < na; i++) {
      const a = (i + 0.5) * da;
      const l = radianceOf(spec, boxes, [Math.cos(a) * Math.cos(el), Math.sin(a) * Math.cos(el), Math.sin(el)]);
      for (let k = 0; k < 3; k++) e[k] += l[k] * w;
    }
  }
  return (0.2126 * e[0] + 0.7152 * e[1] + 0.0722 * e[2]) / Math.PI;
}

/** Khronos PBR Neutral tone mapping (three.js `NeutralToneMapping`), linear in and out. */
export function neutralTone(c: Vec3): Vec3 {
  const start = 0.8 - 0.04;
  const desat = 0.15;
  let [r, g, b] = c.map((v) => Math.max(0, v));
  const x = Math.min(r, g, b);
  const offset = x < 0.08 ? x - 6.25 * x * x : 0.04;
  r -= offset;
  g -= offset;
  b -= offset;
  const peak = Math.max(r, g, b);
  if (peak < start) return [r, g, b];
  const d = 1 - start;
  const np = 1 - (d * d) / (peak + d - start);
  const k = 1 - 1 / (desat * (peak - np) + 1);
  return [r, g, b].map((v) => {
    const w = (v * np) / peak;
    return w + (np - w) * k;
  }) as Vec3;
}

/**
 * Equirectangular image of the environment (`w` × `w/2` RGBA floats, row 0 = v 0) in the
 * convention of three.js `equirectUv`: u = atan2(d.z, d.x) / 2π + ½, v = asin(d.y) / π + ½.
 */
export function environmentPixels(spec: EnvSpec, w: number): Float32Array {
  const h = w / 2;
  const out = new Float32Array(w * h * 4);
  const boxes = placeBoxes(spec);
  for (let y = 0; y < h; y++) {
    const lat = ((y + 0.5) / h - 0.5) * Math.PI;
    for (let x = 0; x < w; x++) {
      const lon = ((x + 0.5) / w - 0.5) * 2 * Math.PI;
      const d: Vec3 = [Math.cos(lat) * Math.cos(lon), Math.sin(lat), Math.cos(lat) * Math.sin(lon)];
      const c = radianceOf(spec, boxes, d);
      const i = (y * w + x) * 4;
      out[i] = c[0];
      out[i + 1] = c[1];
      out[i + 2] = c[2];
      out[i + 3] = 1;
    }
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// CLI call

const n = (v: number) => String(Math.round(v * 1000) / 1000);

/** `--material` value of an entry (`null`: nothing to say). */
function materialArg(m: StudioMaterial): string | null {
  const sel = "part" in m.target ? `part:${m.target.part}` : m.target.color.replace(/^#/, "");
  const parts: string[] = [];
  if (m.preset) parts.push(m.preset);
  if (m.metalness !== null) parts.push(`metalness=${n(m.metalness)}`);
  if (m.roughness !== null) parts.push(`roughness=${n(m.roughness)}`);
  if (m.clearcoat !== null) parts.push(`clearcoat=${n(m.clearcoat)}`);
  if (m.anisotropy !== null) parts.push(`anisotropy=${n(m.anisotropy)}`);
  return parts.length ? `${sel}=${parts.join(",")}` : null;
}

/** Options of `mcad render` for these settings (only what differs from the CLI defaults). The
 *  CLI takes the first light's direction only; more lights need a `render` block. */
export function studioCliArgs(s: StudioSettings): string[] {
  const d = DEFAULT_STUDIO;
  const out = ["--style", "studio"];
  const key = s.lights[0];
  if (key && (key.azimuth !== KEY_LIGHT.azimuth || key.elevation !== KEY_LIGHT.elevation)) out.push("--light", `${n(key.azimuth)},${n(key.elevation)}`);
  if (s.lightIntensity !== d.lightIntensity) out.push("--light-intensity", n(s.lightIntensity));
  if (s.ambient !== d.ambient) out.push("--ambient", n(s.ambient));
  if (s.environment !== d.environment) out.push("--environment", s.environment);
  if (!s.shadows) out.push("--shadows", "off");
  else if (s.shadowSoftness !== d.shadowSoftness) out.push("--shadow-softness", n(s.shadowSoftness));
  if (!s.ao) out.push("--ao", "off");
  else {
    if (s.aoRadius !== null) out.push("--ao-radius", n(s.aoRadius));
    if (s.aoStrength !== d.aoStrength) out.push("--ao-strength", n(s.aoStrength));
  }
  if (!s.ground) out.push("--ground", "off");
  else {
    if (s.groundColor) out.push("--ground-color", s.groundColor.slice(1));
    if (s.groundReflection !== d.groundReflection) out.push("--ground-reflection", n(s.groundReflection));
    if (s.groundRoughness !== d.groundRoughness) out.push("--ground-roughness", n(s.groundRoughness));
  }
  if (!s.reflections) out.push("--reflections", "off");
  if (!s.edges) out.push("--edges", "off");
  for (const m of s.materials) {
    const a = materialArg(m);
    if (a) out.push("--material", a);
  }
  if (s.background) out.push("--background", s.background[0] === s.background[1] ? s.background[0].slice(1) : `${s.background[0].slice(1)},${s.background[1].slice(1)}`);
  return out;
}

/** Full `mcad render` call: studio settings, perspective camera from `eyeDir`, theme. With
 *  `fromFile` (the file has a `render` block with these settings) only the camera is passed. */
export function studioCliCommand(s: StudioSettings, file: string, module: string | null, eyeDir: Vec3 | null, theme: "light" | "dark", output = "studio.png", fromFile = false): string {
  const q = (t: string) => (/^[A-Za-z0-9_./:=,-]+$/.test(t) ? t : `'${t.replace(/'/g, "'\\''")}'`);
  const parts = ["mcad", "render", q(file), "-o", q(output)];
  if (module) parts.push("--module", q(module));
  if (eyeDir) {
    const c = cliCamera(eyeDir);
    parts.push("--perspective", "--azimuth", n(c.azimuth), "--elevation", n(c.elevation));
  }
  if (theme === "dark") parts.push("--theme", "dark");
  parts.push(...(fromFile ? ["--style", "studio"] : studioCliArgs(s)).map(q));
  return parts.join(" ");
}
