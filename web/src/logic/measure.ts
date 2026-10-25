// Measure mode (Plan 18): kinds of the context bar, texts for copying (plain, code comment,
// `mcad measure` call) and small helpers. The measuring itself happens in the engine
// (`cad_session::measure`, `cad-measure`); these functions only format its answers.

import type { MeasureKind, Measurement } from "../engine/protocol";

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
export const MEASURE_KINDS: KindInfo[] = [
  { kind: "distance", label: "Abstand", title: "Kleinster Abstand zweier Elemente mit Fußpunkten (parallele Ebenen: Normalabstand)", key: "D", where: "both" },
  { kind: "radius", label: "Radius", title: "Radius und Durchmesser einer Kreiskante oder runden Fläche, mit Mittelpunkt und Achse", key: "R", where: "both" },
  { kind: "angle", label: "Winkel", title: "Winkel zwischen zwei Kanten oder Flächen", key: "W", where: "both" },
  { kind: "length", label: "Länge", title: "Länge einer Kante (im Schnitt: eines Konturabschnitts)", key: "L", where: "both" },
  { kind: "area", label: "Fläche", title: "Fläche einer Fläche (im Schnitt: eines Bereichs der Schnittfläche)", key: "F", where: "both" },
  { kind: "body", label: "Körper", title: "Hülle, Volumen und Oberfläche des Körpers bzw. Bauteils", key: "K", where: "model" },
  { kind: "wall", label: "Wandstärke", title: "Wandstärke quer zur Schnittkontur an einer Stelle", key: "T", where: "section" },
];

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
  return lines.map((l, i) => (i === 0 ? `// Messung: ${l}` : `//   ${l.trim()}`)).join("\n");
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
  if (m.exact) return "exakt";
  const t = m.tol ?? 0;
  return t > 0 ? `± ${t < 1e-3 ? t.toExponential(1) : Number(t.toPrecision(3))} mm` : "genähert";
}

/** Section plane of a measurement as text (`z = 10`). */
export function sectionText(m: Measurement): string | null {
  const s = m.section;
  if (!s) return null;
  const n = s.normal;
  const axis = [0, 1, 2].find((k) => Math.abs(Math.abs(n[k]) - 1) < 1e-9);
  if (axis !== undefined) return `${"xyz"[axis]} = ${Number(s.point[axis].toFixed(4))}`;
  return "freie Ebene";
}
