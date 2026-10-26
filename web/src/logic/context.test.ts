import { describe, expect, it } from "vitest";
import { definitionTarget, jumpUpTarget, moduleSpans, pathSegments } from "./context";
import { SceneIndex } from "./sceneIndex";
import { fixture, pos } from "./__fixtures__/load";

describe("Module context and path bar", () => {
  const f = fixture("housing-module");
  const idx = new SceneIndex(f.scene);
  const mods = moduleSpans(idx);

  it("path bar from the context", () => {
    expect(pathSegments(f.scene, "housing.mcad")).toEqual(["housing.mcad", "housing"]);
    expect(pathSegments(null, "x.scad")).toEqual(["x.scad"]);
  });

  it("one level up: the module's call in the file", () => {
    const p = jumpUpTarget(f.source, mods, ["housing"], 0)!;
    expect(f.doc.lineAt(p).number).toBe(21);
    expect(f.source.slice(p, p + 8)).toBe("housing(");
  });

  it("without a call: behind the definition", () => {
    const src = f.source.replace("k: housing(w = 60);", "k: cube(1);");
    const p = jumpUpTarget(src, mods, ["housing"], 0)!;
    expect(p).toBe(mods.find((m) => m.name === "housing")!.to + 1);
  });

  it("calls in comments and definitions do not count", () => {
    const src = "// housing(1)\nmodule housing() { cube(1); }\nmodule x() { housing(); }\nhousing();\n";
    const m = [
      { name: "housing", from: 14, to: 43, body: { from: 31, to: 43 }, path: [] },
      { name: "x", from: 44, to: 70, body: { from: 55, to: 70 }, path: [] },
    ];
    expect(jumpUpTarget(src, m, ["housing"], 0)).toBe(src.lastIndexOf("housing();"));
  });

  it("definition: jump into the body", () => {
    const mod = f.scene.modules.find((m) => m.name === "housing")!;
    const p = definitionTarget(f.source, idx.range(mod.body_range));
    expect(f.doc.lineAt(p).number).toBe(4);
    expect(p).toBe(pos(f.doc, 4, 4));
    const lid = f.scene.modules.find((m) => m.name === "lid")!;
    expect(f.source.slice(definitionTarget(f.source, idx.range(lid.body_range))).startsWith("cube(")).toBe(true);
  });
});
