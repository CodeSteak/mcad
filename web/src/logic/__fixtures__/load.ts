// Test fixtures: scenes of the example files, evaluated with the browser engine in Node
// (regenerate: node web/scripts/dump-scene.mjs <file> [--module m] > <fixture>.json).

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { Text } from "@codemirror/state";
import type { Diagnostic, Scene } from "../../engine/protocol";

const FIXTURES: Record<string, { example: string; local?: boolean }> = {
  griffe: { example: "griffe.scad", local: true },
  profile: { example: "profile.scad", local: true },
  gehaeuse: { example: "spec/gehaeuse.scad" },
  "gehaeuse-modul": { example: "spec/gehaeuse.scad" },
  modifiers: { example: "openscad-compat/24_modifiers.scad" },
  "root-modifier": { example: "openscad-compat/25_root_modifier.scad" },
  "lochplatte-modul": { example: "spec/lochplatte.scad" },
  "fillet-modul": { example: "spec/fillet_zonen.scad" },
  winkel: { example: "spec/winkel_anker.scad" },
  nested: { example: "openscad-compat/05_nested_booleans.scad" },
  "spacers-2d": { example: "openscad-compat/27_spacers_2d.scad" },
  projekt: { example: "projekt-gehaeuse/gehaeuse.scad" },
  "projekt-modul": { example: "projekt-gehaeuse/gehaeuse.scad" },
  skizze: { example: "spec/skizze_profil.scad" },
  "skizze-modul": { example: "spec/skizze_profil.scad" },
  bibliothek: { example: "spec/skizze_bibliothek.scad" },
};

export interface Fixture {
  scene: Scene;
  diagnostics: Diagnostic[];
  source: string;
  doc: Text;
}

export function fixture(name: keyof typeof FIXTURES): Fixture {
  const f = JSON.parse(readFileSync(join(__dirname, `${name}.json`), "utf8")) as { scene: Scene; diagnostics: Diagnostic[] };
  const fx = FIXTURES[name];
  const source = readFileSync(fx.local ? join(__dirname, fx.example) : join(__dirname, "../../../../examples", fx.example), "utf8");
  return { ...f, source, doc: Text.of(source.split("\n")) };
}

/** Position of a 1-based line and 0-based column. */
export function pos(doc: Text, line: number, col = 0): number {
  const l = doc.line(line);
  return Math.min(l.from + col, l.to);
}
