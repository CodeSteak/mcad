// Studio look of the 3D view (like `mcad render --style studio`): settings, defaults, the light
// direction relative to the camera and the equivalent CLI call. Pure functions (unit tests).

import type { Vec3 } from "../engine/protocol";
import { cliCamera } from "./section";

export interface StudioSettings {
  /** Studio look on. */
  on: boolean;
  /** Key light azimuth in degrees relative to the camera (0 = from the camera, + = from the right). */
  lightAzimuth: number;
  /** Key light elevation in degrees above the xy plane. */
  lightElevation: number;
  /** Key and fill light, 0…4. */
  lightIntensity: number;
  /** Share of the sky light, 0…1. */
  ambient: number;
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
  /** Background gradient top, bottom (`#rrggbb`, equal = solid); `null` = from the theme. */
  background: [string, string] | null;
}

/** Defaults: the same as `mcad render --style studio`. */
export const DEFAULT_STUDIO: StudioSettings = {
  on: false,
  lightAzimuth: -40,
  lightElevation: 50,
  lightIntensity: 1,
  ambient: 0.4,
  shadows: true,
  shadowSoftness: 0.35,
  ao: true,
  aoRadius: null,
  aoStrength: 1,
  ground: true,
  groundColor: null,
  background: null,
};

/** Theme colours of the studio look (as the CLI's `--theme light|dark`). */
export const STUDIO_THEME = {
  light: { ground: "#e6e8ec", background: ["#f8f9fb", "#d9dde3"] as [string, string] },
  dark: { ground: "#2c3038", background: ["#2b3038", "#111317"] as [string, string] },
};

const HEX = /^#[0-9a-f]{6}$/;

/** Settings from stored JSON: unknown or out-of-range values fall back to the defaults. */
export function sanitizeStudio(raw: unknown): StudioSettings {
  const d = DEFAULT_STUDIO;
  if (!raw || typeof raw !== "object") return { ...d };
  const r = raw as Record<string, unknown>;
  const num = (k: keyof StudioSettings, lo: number, hi: number): number => {
    const v = r[k];
    return typeof v === "number" && Number.isFinite(v) && v >= lo && v <= hi ? v : (d[k] as number);
  };
  const bool = (k: keyof StudioSettings): boolean => (typeof r[k] === "boolean" ? (r[k] as boolean) : (d[k] as boolean));
  const hex = (v: unknown): string | null => (typeof v === "string" && HEX.test(v.toLowerCase()) ? v.toLowerCase() : null);
  const bg = Array.isArray(r.background) && r.background.length === 2 ? [hex(r.background[0]), hex(r.background[1])] : null;
  return {
    on: bool("on"),
    lightAzimuth: num("lightAzimuth", -360, 360),
    lightElevation: num("lightElevation", 1, 90),
    lightIntensity: num("lightIntensity", 0, 4),
    ambient: num("ambient", 0, 1),
    shadows: bool("shadows"),
    shadowSoftness: num("shadowSoftness", 0, 1),
    ao: bool("ao"),
    aoRadius: typeof r.aoRadius === "number" && Number.isFinite(r.aoRadius) && r.aoRadius > 0 ? r.aoRadius : null,
    aoStrength: num("aoStrength", 0, 2),
    ground: bool("ground"),
    groundColor: hex(r.groundColor),
    background: bg && bg[0] && bg[1] ? [bg[0], bg[1]] : null,
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
  let camAz: number;
  if (Math.abs(eye[0]) < 1e-9 && Math.abs(eye[1]) < 1e-9) {
    const s = Math.sign(eye[2]) || 1;
    const e = [-up[0] * s, -up[1] * s];
    camAz = Math.atan2(e[0], -e[1]);
  } else camAz = Math.atan2(eye[0], -eye[1]);
  const a = camAz + (az * Math.PI) / 180;
  const elc = Math.max(-89.9, Math.min(89.9, el));
  const e = (elc * Math.PI) / 180;
  return [Math.sin(a) * Math.cos(e), -Math.cos(a) * Math.cos(e), Math.sin(e)];
}

const n = (v: number) => String(Math.round(v * 1000) / 1000);

/** Options of `mcad render` for these settings (only what differs from the CLI defaults). */
export function studioCliArgs(s: StudioSettings): string[] {
  const d = DEFAULT_STUDIO;
  const out = ["--style", "studio"];
  if (s.lightAzimuth !== d.lightAzimuth || s.lightElevation !== d.lightElevation) out.push("--light", `${n(s.lightAzimuth)},${n(s.lightElevation)}`);
  if (s.lightIntensity !== d.lightIntensity) out.push("--light-intensity", n(s.lightIntensity));
  if (s.ambient !== d.ambient) out.push("--ambient", n(s.ambient));
  if (!s.shadows) out.push("--shadows", "off");
  else if (s.shadowSoftness !== d.shadowSoftness) out.push("--shadow-softness", n(s.shadowSoftness));
  if (!s.ao) out.push("--ao", "off");
  else {
    if (s.aoRadius !== null) out.push("--ao-radius", n(s.aoRadius));
    if (s.aoStrength !== d.aoStrength) out.push("--ao-strength", n(s.aoStrength));
  }
  if (!s.ground) out.push("--ground", "off");
  else if (s.groundColor) out.push("--ground-color", s.groundColor.slice(1));
  if (s.background) out.push("--background", s.background[0] === s.background[1] ? s.background[0].slice(1) : `${s.background[0].slice(1)},${s.background[1].slice(1)}`);
  return out;
}

/** Full `mcad render` call: studio settings, perspective camera from `eyeDir`, theme. */
export function studioCliCommand(s: StudioSettings, file: string, module: string | null, eyeDir: Vec3 | null, theme: "light" | "dark", output = "studio.png"): string {
  const q = (t: string) => (/^[A-Za-z0-9_./:-]+$/.test(t) ? t : `'${t.replace(/'/g, "'\\''")}'`);
  const parts = ["mcad", "render", q(file), "-o", q(output)];
  if (module) parts.push("--module", q(module));
  if (eyeDir) {
    const c = cliCamera(eyeDir);
    parts.push("--perspective", "--azimuth", n(c.azimuth), "--elevation", n(c.elevation));
  }
  if (theme === "dark") parts.push("--theme", "dark");
  parts.push(...studioCliArgs(s));
  return parts.join(" ");
}
