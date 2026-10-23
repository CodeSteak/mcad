// Number formatting for text written into the source (same rules as `fmt_literal` in
// cad-eval): rounded to a multiple of the step, shortest form, no trailing zeros, never `-0`.

/** Number of decimals of the shortest representation of a step. */
export function stepDecimals(step: number): number {
  const s = String(step);
  if (s.includes("e-")) return Math.min(10, Number(s.split("e-")[1]) + (s.split("e-")[0].split(".")[1]?.length ?? 0));
  const d = s.split(".")[1];
  return d ? Math.min(10, d.length) : 0;
}

/** Rounds to a multiple of `step` (no rounding for `step <= 0`). */
export function roundTo(v: number, step: number): number {
  if (!(step > 0)) return v;
  const r = Math.round(v / step) * step;
  return Number(r.toFixed(stepDecimals(step)));
}

function trimZeros(s: string): string {
  const t = s.includes(".") ? s.replace(/0+$/, "").replace(/\.$/, "") : s;
  return t === "-0" ? "0" : t;
}

/** Literal text of a number: rounded to `step`, at most 6 decimals without a step. */
export function fmtNum(v: number, step = 0): string {
  if (!Number.isFinite(v)) return "0";
  if (step > 0) return trimZeros(roundTo(v, step).toFixed(stepDecimals(step)));
  return trimZeros(v.toFixed(6));
}

/** Source text of a parameter value (numbers, bools, vectors). */
export function valueText(v: unknown): string {
  if (typeof v === "number") return fmtNum(v);
  if (typeof v === "boolean") return v ? "true" : "false";
  if (typeof v === "string") return JSON.stringify(v);
  if (Array.isArray(v)) return `[${v.map(valueText).join(", ")}]`;
  return "undef";
}

/** Angle closest to `prev` that is equal to `deg` modulo 360. */
export function unwrapDeg(deg: number, prev: number): number {
  let d = deg;
  while (d - prev > 180) d -= 360;
  while (d - prev < -180) d += 360;
  return d;
}
