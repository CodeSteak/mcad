// Test fixtures: scenes of the example files, evaluated with the browser engine in Node
// (regenerate: node web/scripts/dump-scene.mjs <file> --lang de [--module m] [--project] > <fixture>.json).

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { Text } from "@codemirror/state";
import type { Diagnostic, Scene } from "../../engine/protocol";

const FIXTURES: Record<string, { example: string; local?: boolean }> = {
  handles: { example: "handles.mcad", local: true },
  profile: { example: "profile.mcad", local: true },
  housing: { example: "spec/housing.mcad" },
  "housing-module": { example: "spec/housing.mcad" },
  modifiers: { example: "openscad-compat/24_modifiers.scad" },
  "root-modifier": { example: "openscad-compat/25_root_modifier.scad" },
  "hole-plate-module": { example: "spec/hole_plate.mcad" },
  "fillet-module": { example: "spec/fillet_zones.mcad" },
  "angle-anchor": { example: "spec/angle_anchor.mcad" },
  nested: { example: "openscad-compat/05_nested_booleans.scad" },
  "spacers-2d": { example: "openscad-compat/27_spacers_2d.scad" },
  project: { example: "project-housing/housing.mcad" },
  "project-module": { example: "project-housing/housing.mcad" },
  sketch: { example: "spec/sketch_profile.mcad" },
  "sketch-module": { example: "spec/sketch_profile.mcad" },
  library: { example: "spec/sketch_library.mcad" },
  "sketch-lid": { example: "spec/sketch_lid.mcad" },
  "sketch-holder": { example: "spec/sketch_holder.mcad" },
  "sketch-cable-duct": { example: "spec/sketch_cable_duct.mcad" },
  "sketch-path": { example: "spec/sketch_cable_duct.mcad" },
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
  const source = readFileSync(fx.local ? join(__dirname, fx.example) : join(__dirname, "../../../../tests/models", fx.example), "utf8");
  return { ...f, source, doc: Text.of(source.split("\n")) };
}

/** Position of a 1-based line and 0-based column. */
export function pos(doc: Text, line: number, col = 0): number {
  const l = doc.line(line);
  return Math.min(l.from + col, l.to);
}
