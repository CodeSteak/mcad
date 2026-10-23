// Parameter bar (Spec: Parameterleiste): rows from the module signature, temporary values per
// module (they only change the preview, never the code), reset values, changed markers and
// slider ranges. Pure functions; the store keeps `ParamOverrides`.

import type { Module, Param, ParamValue, Scene, Value } from "../engine/protocol";

/** Temporary values: module name → parameter → value. Kept until the session ends. */
export type ParamOverrides = Record<string, Record<string, Value>>;

export type ControlKind = "number" | "bool" | "vector" | "other";

export interface ParamRow {
  name: string;
  control: ControlKind;
  /** Value shown (override, else the value the engine used, else the reset value). */
  value: Value;
  /** Reset target: the default from the definition, else 0 / false / [0, 0, 0]. */
  reset: Value;
  hasDefault: boolean;
  defaultText: string | null;
  /** A temporary value differs from the reset value. */
  changed: boolean;
  overridden: boolean;
  /** Slider range for numbers. */
  min: number;
  max: number;
  step: number;
}

function isNumVec(v: Value): v is number[] {
  return Array.isArray(v) && v.length >= 2 && v.length <= 3 && v.every((x) => typeof x === "number");
}

export function controlOf(p: Param, used: Value | undefined): ControlKind {
  switch (p.ty) {
    case "number":
      return "number";
    case "bool":
      return "bool";
    case "vector":
      return "vector";
    case "unknown": {
      const v = used;
      if (typeof v === "boolean") return "bool";
      if (v !== undefined && v !== null && isNumVec(v)) return "vector";
      if (typeof v === "string" || (Array.isArray(v) && !isNumVec(v))) return "other";
      return "number";
    }
    default:
      return "other";
  }
}

/** Reset value of a parameter (Spec table: 0, false, [0, 0, 0] without a default). */
export function resetValue(p: Param, control: ControlKind): Value {
  if (p.default_value !== null && p.default_value !== undefined) return p.default_value;
  if (control === "bool") return false;
  if (control === "vector") return [0, 0, 0];
  if (control === "number") return 0;
  return null;
}

export function sameValue(a: Value | undefined, b: Value | undefined): boolean {
  if (typeof a === "number" && typeof b === "number") return Math.abs(a - b) <= 1e-12 * Math.max(1, Math.abs(a), Math.abs(b));
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((x, i) => sameValue(x, b[i]));
  return JSON.stringify(a ?? null) === JSON.stringify(b ?? null);
}

/** Slider range around a reference value. */
export function sliderRange(ref: number): { min: number; max: number; step: number } {
  const a = Math.abs(ref);
  const step = Number.isInteger(ref) || a >= 20 ? 1 : a >= 2 ? 0.5 : 0.1;
  if (ref >= 0) return { min: 0, max: Math.max(10, Math.ceil((ref * 2) / step) * step), step };
  return { min: Math.min(-10, Math.floor((ref * 2) / step) * step), max: Math.max(10, -ref), step };
}

export function paramRows(sig: Module, used: ParamValue[], overrides: Record<string, Value> | undefined): ParamRow[] {
  return sig.params.map((p) => {
    const u = used.find((x) => x.name === p.name)?.value;
    const control = controlOf(p, u);
    const reset = resetValue(p, control);
    const ov = overrides && Object.prototype.hasOwnProperty.call(overrides, p.name) ? overrides[p.name] : undefined;
    const value = ov !== undefined ? ov : u !== undefined ? u : reset;
    const ref = typeof reset === "number" ? reset : typeof value === "number" ? value : 0;
    const r = sliderRange(ref);
    const v = typeof value === "number" ? value : ref;
    return {
      name: p.name,
      control,
      value,
      reset,
      hasDefault: p.default_value !== null && p.default_value !== undefined,
      defaultText: p.default_text,
      changed: ov !== undefined && !sameValue(ov, reset),
      overridden: ov !== undefined,
      min: Math.min(r.min, v),
      max: Math.max(r.max, v),
      step: r.step,
    };
  });
}

/** Module signature of the context module of a scene. */
export function contextModule(scene: Scene | null): Module | null {
  if (!scene || scene.context.module === null) return null;
  const path = scene.context.path;
  const name = scene.context.module.split("::").pop()!;
  return (
    scene.modules.find((m) => m.name === name && m.path.join("::") === path.slice(0, -1).join("::")) ??
    scene.modules.find((m) => m.name === name) ??
    null
  );
}

/** New overrides after setting `param` of `module` to `value`. */
export function setOverride(all: ParamOverrides, module: string, param: string, value: Value): ParamOverrides {
  return { ...all, [module]: { ...(all[module] ?? {}), [param]: value } };
}

/**
 * New overrides after a reset: parameters with a default lose their temporary value; those
 * without get 0 / false / [0, 0, 0] (otherwise they would be undef).
 */
export function resetOverride(all: ParamOverrides, module: string, p: Param, control: ControlKind): ParamOverrides {
  const cur = { ...(all[module] ?? {}) };
  if (p.default_value !== null && p.default_value !== undefined) delete cur[p.name];
  else cur[p.name] = resetValue(p, control);
  return { ...all, [module]: cur };
}

export function resetAll(all: ParamOverrides, sig: Module, used: ParamValue[]): ParamOverrides {
  let out = { ...all, [sig.name]: {} };
  for (const p of sig.params) {
    if (p.default_value === null || p.default_value === undefined) {
      const u = used.find((x) => x.name === p.name)?.value;
      out = resetOverride(out, sig.name, p, controlOf(p, u));
    }
  }
  return out;
}

/** Drops a temporary value (after "Als Standard übernehmen"). */
export function dropOverride(all: ParamOverrides, module: string, param: string): ParamOverrides {
  const cur = { ...(all[module] ?? {}) };
  delete cur[param];
  return { ...all, [module]: cur };
}
