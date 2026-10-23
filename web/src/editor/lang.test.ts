import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { disabledStatements, errorNodes, scadParser } from "./lang";

const root = join(__dirname, "../../../examples");
const files = ["spec", "openscad-compat"].flatMap((d) =>
  readdirSync(join(root, d))
    .filter((f) => f.endsWith(".scad"))
    .map((f) => join(d, f)),
);

describe("Lezer-Grammatik", () => {
  it.each(files)("parst %s ohne Fehler", (f) => {
    const text = readFileSync(join(root, f), "utf8");
    const tree = scadParser.parse(text);
    const errs = errorNodes(tree).map((p) => {
      const line = text.slice(0, p).split("\n").length;
      return `${line}: ${text.split("\n")[line - 1]}`;
    });
    expect(errs).toEqual([]);
  });

  it("erkennt Raster, Anker, Label und Modifier", () => {
    const text =
      "grid g { x: a = 0, b = w - t; }\nk: gehaeuse(w = 60);\nanchor e = g[a, *, _], dir = [0, 0, 1];\n* translate([1, 0, 0]) cube(2);\n";
    const tree = scadParser.parse(text);
    const names: string[] = [];
    tree.iterate({ enter: (n) => void names.push(n.name) });
    for (const n of ["GridDecl", "GridAxis", "PlaneName", "Label", "AnchorDecl", "Wildcard", "Modifier"]) {
      expect(names).toContain(n);
    }
    expect(errorNodes(tree)).toEqual([]);
    const dis = disabledStatements(tree, text);
    expect(dis).toHaveLength(1);
    expect(text.slice(dis[0].from, dis[0].to)).toBe("* translate([1, 0, 0]) cube(2);");
  });
});
