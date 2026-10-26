// Measure mode (Plan 18): kinds of the context bar, texts for copying (plain, code comment,
// `mcad measure` call) and small helpers. The measuring itself happens in the engine
// (`cad_session::measure`, `cad-measure`); these functions only format its answers.

import type { MeasureKind, Measurement } from "../engine/protocol";
import { t } from "../i18n";

export interface KindInfo {
  kind: MeasureKind;
  label: string;
  title: string;
  /** Keyboard shortcut in the measure mode. */
  key: string;
  /** Where the kind is offered: on the model, in the section or both. */
  where: "both" | "model" | "section";
}

/** Kinds in the order of the context bar (Abstand, Radius, Winkel first, as in the UI concept). */
// (the keys stay the same in every language)
export const MEASURE_KINDS: KindInfo[] = (
  [
    ["distance", "D", "both"],
    ["radius", "R", "both"],
    ["angle", "W", "both"],
    ["length", "L", "both"],
    ["area", "F", "both"],
    ["body", "K", "model"],
    ["wall", "T", "section"],
  ] as const
).map(([kind, key, where]) => ({
  kind,
  key,
  where,
  get label() {
    return t(`measure.kind.${kind}`);
  },
  get title() {
    return t(`measure.kind.${kind}.title`);
  },
}));

export function kindAvailable(kind: MeasureKind, inSection: boolean): boolean {
  const k = MEASURE_KINDS.find((x) => x.kind === kind);
  if (!k) return false;
  return k.where === "both" || (k.where === "section") === inSection;
}

/** A kind usable in the mode (keeps `kind` if it fits, else Abstand). */
export function fitKind(kind: MeasureKind, inSection: boolean): MeasureKind {
  return kindAvailable(kind, inSection) ? kind : "distance";
}

/** Shell quoting for copied commands (POSIX shells). */
export function shellQuote(t: string): string {
  return /^[A-Za-z0-9_./:=,@+-]+$/.test(t) ? t : `'${t.replace(/'/g, "'\\''")}'`;
}

/** `mcad measure DATEI …` that repeats the measurement (arguments from the engine). */
export function measureCli(m: Measurement, file: string): string {
  return ["mcad", "measure", shellQuote(file), ...m.cli.map(shellQuote)].join(" ");
}

/** The measurement as a code comment (`// Messung: …`, one line per line of text). */
export function measureComment(m: Measurement): string {
  const lines = m.text.split("\n");
  return lines.map((l, i) => (i === 0 ? `// ${t("measure.comment")}: ${l}` : `//   ${l.trim()}`)).join("\n");
}

/** Text of all measurements (one block each). */
export function listText(ms: Measurement[]): string {
  return ms.map((m) => m.text).join("\n\n");
}

/** Short value text of a measurement (`12.5 mm`, `45°`). */
export function valueText(m: Measurement): string {
  return m.label;
}

/** Accuracy note (`exakt` or `± 0.02 mm`). */
export function accuracyText(m: Measurement): string {
  if (m.exact) return t("measure.exact");
  const tol = m.tol ?? 0;
  return tol > 0 ? `± ${tol < 1e-3 ? tol.toExponential(1) : Number(tol.toPrecision(3))} mm` : t("measure.approx");
}

/** Section plane of a measurement as text (`z = 10`). */
export function sectionText(m: Measurement): string | null {
  const s = m.section;
  if (!s) return null;
  const n = s.normal;
  const axis = [0, 1, 2].find((k) => Math.abs(Math.abs(n[k]) - 1) < 1e-9);
  if (axis !== undefined) return `${"xyz"[axis]} = ${Number(s.point[axis].toFixed(4))}`;
  return t("measure.freePlane");
}
