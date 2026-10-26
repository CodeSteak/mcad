import { describe, expect, it } from "vitest";
import { definitionTarget, jumpUpTarget, moduleSpans, pathSegments } from "./context";
import { SceneIndex } from "./sceneIndex";
import { fixture, pos } from "./__fixtures__/load";

describe("Modulkontext und Pfadleiste", () => {
  const f = fixture("housing-module");
  const idx = new SceneIndex(f.scene);
  const mods = moduleSpans(idx);

  it("Pfadleiste aus dem Kontext", () => {
    expect(pathSegments(f.scene, "housing.mcad")).toEqual(["housing.mcad", "housing"]);
    expect(pathSegments(null, "x.scad")).toEqual(["x.scad"]);
  });

  it("eine Ebene hoch: Aufruf des Moduls in der Datei", () => {
    const p = jumpUpTarget(f.source, mods, ["housing"], 0)!;
    expect(f.doc.lineAt(p).number).toBe(21);
    expect(f.source.slice(p, p + 8)).toBe("housing(");
  });

  it("ohne Aufruf: hinter die Definition", () => {
    const src = f.source.replace("k: housing(w = 60);", "k: cube(1);");
    const p = jumpUpTarget(src, mods, ["housing"], 0)!;
    expect(p).toBe(mods.find((m) => m.name === "housing")!.to + 1);
  });

  it("Aufrufe in Kommentaren und Definitionen zählen nicht", () => {
    const src = "// gehaeuse(1)\nmodule gehaeuse() { cube(1); }\nmodule x() { gehaeuse(); }\ngehaeuse();\n";
    const m = [
      { name: "gehaeuse", from: 15, to: 45, body: { from: 33, to: 45 }, path: [] },
      { name: "x", from: 46, to: 73, body: { from: 57, to: 73 }, path: [] },
    ];
    expect(jumpUpTarget(src, m, ["gehaeuse"], 0)).toBe(src.lastIndexOf("gehaeuse();"));
  });

  it("Definition: Sprung in den Rumpf", () => {
    const mod = f.scene.modules.find((m) => m.name === "housing")!;
    const p = definitionTarget(f.source, idx.range(mod.body_range));
    expect(f.doc.lineAt(p).number).toBe(4);
    expect(p).toBe(pos(f.doc, 4, 4));
    const lid = f.scene.modules.find((m) => m.name === "lid")!;
    expect(f.source.slice(definitionTarget(f.source, idx.range(lid.body_range))).startsWith("cube(")).toBe(true);
  });
});
