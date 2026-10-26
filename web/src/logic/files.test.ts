import { readFileSync } from "node:fs";
import { join } from "node:path";
import { Text } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { collectLines } from "../editor/decorations";
import { VersionMapper } from "./positions";
import { visiblePieces, plan } from "./display";
import { dataFileRefs, dataUrl, fileBytes, fileIds, fileText, fromDataUrl, guessMain, includeAt, includeStatements, isBinaryDataFile, isDataFile, isFontFile, isMeshFile, isModelFile, isProjectFile, normalizePath, rangePath, resolvePath, stripCommonFolder, stripModelExt, withModelExt } from "./files";
import { SceneIndex, isOff } from "./sceneIndex";
import { fixture } from "./__fixtures__/load";

const EX = join(__dirname, "../../../tests/models/project-housing");
const read = (p: string) => readFileSync(join(EX, p), "utf8");

describe("Paths like the engine", () => {
  it("relative to the including file", () => {
    expect(resolvePath("main.scad", "lib/a.scad")).toBe("lib/a.scad");
    expect(resolvePath("lib/a.scad", "b.scad")).toBe("lib/b.scad");
    expect(resolvePath("lib/a.scad", "../consts.scad")).toBe("consts.scad");
    expect(resolvePath("lib/a.scad", "./x/../y.scad")).toBe("lib/y.scad");
    expect(resolvePath("main.scad", "../outside.scad")).toBe("../outside.scad");
    expect(resolvePath("a\\b.scad", "c\\d.scad")).toBe("a/c/d.scad");
    expect(normalizePath("/a/./b/../c")).toBe("/a/c");
  });

  it("include/use in the text, not in comments", () => {
    const t = "// include <no.scad>\ninclude <consts.scad>\n/* use <x> */ use <lib/parts.mcad>;";
    expect(includeStatements(t).map((s) => [s.path, s.isUse])).toEqual([
      ["consts.scad", false],
      ["lib/parts.mcad", true],
    ]);
    const at = t.indexOf("consts");
    expect(includeAt(t, at)?.path).toBe("consts.scad");
    expect(includeAt(t, 3)).toBeNull();
  });

  it("main file of a folder", () => {
    const files = { "housing.mcad": read("housing.mcad"), "constants.mcad": read("constants.mcad"), "lib/parts.mcad": read("lib/parts.mcad") };
    expect(guessMain(files)).toBe("housing.mcad");
    expect(guessMain({ "b.scad": "cube(1);", "a.scad": "include <b.scad>" })).toBe("a.scad");
    expect(guessMain({ "x.txt": "" })).toBeNull();
    // .mcad and .scad mixed in one project
    expect(guessMain({ "lib/a.scad": "module a() {}", "part.mcad": "use <lib/a.scad>\na();", "note.txt": "" })).toBe("part.mcad");
    expect(guessMain({ "old.SCAD": "use <new.mcad>", "new.mcad": "module n() {}" })).toBe("old.SCAD");
  });

  it("model files: .mcad new, .scad still", () => {
    expect(["a.mcad", "a.scad", "lib/B.MCAD", "c.Scad"].every(isModelFile)).toBe(true);
    expect(["a.svg", "a.mcad.txt", "scad", "a.cad"].some(isModelFile)).toBe(false);
    expect(isProjectFile("x/y.scad") && isProjectFile("x/y.mcad") && isProjectFile("logo.svg")).toBe(true);
    expect(withModelExt("lib/parts")).toBe("lib/parts.mcad");
    expect(withModelExt("old.scad")).toBe("old.scad");
    expect(stripModelExt("part.mcad")).toBe("part");
    expect(stripModelExt("part.scad")).toBe("part");
    expect(stripCommonFolder({ "p/a.scad": "1", "p/lib/b.scad": "2" })).toEqual({ files: { "a.scad": "1", "lib/b.scad": "2" }, folder: "p" });
    expect(stripCommonFolder({ "a.scad": "1", "p/b.scad": "2" }).folder).toBeNull();
  });
});

describe("Scene of a project", () => {
  const f = fixture("project");
  const s = f.scene;

  it("files, parts, include statements", () => {
    expect(s.files.map((x) => [x.id, x.path, x.role])).toEqual([
      [0, "housing.mcad", "main"],
      [1, "constants.mcad", "include"],
      [2, "lib/parts.mcad", "use"],
    ]);
    expect(s.parts.map((p) => p.name)).toEqual(["housing", "lid"]);
    expect(s.includes.map((i) => [i.target, i.is_use, i.found])).toEqual([
      ["constants.mcad", false, true],
      ["lib/parts.mcad", true, true],
    ]);
    expect(fileIds(s, "lib/parts.mcad", "housing.mcad")).toEqual(new Set([2]));
    expect(fileIds(s, "housing.mcad", "housing.mcad")).toEqual(new Set([0]));
  });

  it("index of the main file: ranges of other files do not count", () => {
    const idx = new SceneIndex(s, undefined, fileIds(s, "housing.mcad", "housing.mcad"));
    const hint = s.hints.find((h) => h.text === "60")!;
    expect(hint.range.file).toBe(1);
    expect(isOff(idx.range(hint.range))).toBe(true);
    const call = s.nodes.find((n) => n.instance === "housing")!;
    expect(idx.linesOf(f.doc, [call.idx])).toEqual([6]);
    // A node of the library maps to no line of the main file.
    const libNode = s.nodes.find((n) => n.range.file === 2)!;
    expect(idx.linesOf(f.doc, [libNode.idx])).toEqual([]);
  });

  it("index of the library: modules and lines in lib/parts.mcad", () => {
    const lib = read("lib/parts.mcad");
    const doc = Text.of(lib.split("\n"));
    const m = fixture("project-module").scene;
    const idx = new SceneIndex(m, undefined, fileIds(m, "lib/parts.mcad", "housing.mcad"));
    const pos = doc.line(17).from + 6;
    expect(idx.moduleAt(pos)).toBe("housing");
    const sel = idx.select(doc, doc.line(21).from + 10);
    expect(sel.nodes.length).toBeGreaterThan(0);
    expect(rangePath(m, m.nodes[sel.nodes[0]].range, "housing.mcad")).toBe("lib/parts.mcad");
  });

  it("decorations only for the file in the editor", () => {
    const consts = read("constants.mcad");
    const doc = Text.of(consts.split("\n"));
    const base = { complete: null, diagnostics: { version: 0, items: [] }, mapper: new VersionMapper(doc.length), engineError: null, viewHoverLines: [], hoverLine: null, hoverRef: null };
    const inConsts = collectLines(doc, { ...base, scene: s, activeFile: "constants.mcad", mainFile: "housing.mcad" }, []);
    expect(inConsts.lines.get(2)?.segs).toEqual([{ kind: "hint", text: "= 60" }]);
    // The same scene seen from the main file: no hints of constants.mcad on its lines.
    const main = Text.of(f.source.split("\n"));
    const inMain = collectLines(main, { ...base, scene: s, activeFile: "housing.mcad", mainFile: "housing.mcad" }, []);
    expect([...inMain.lines.values()].flatMap((l) => l.segs).filter((x) => x.kind === "hint")).toEqual([]);
    // Diagnostics name their file.
    const d = { severity: "error" as const, message: "x", source: "eval" as const, node: null, file: "lib/parts.mcad", range: { from: 0, to: 1, line: 1, col: 1, end_line: 1, end_col: 2 } };
    const lines = collectLines(main, { ...base, scene: null, diagnostics: { version: 0, items: [d] }, activeFile: "housing.mcad", mainFile: "housing.mcad" }, []).lines;
    expect(lines.size).toBe(0);
  });

  it("hidden parts: model in pieces without them", () => {
    const lid = s.parts.find((p) => p.name === "lid")!;
    const pieces = visiblePieces(s, [lid.node]);
    const labels = pieces.map((n) => n.instance ?? n.label);
    expect(labels).toEqual(["housing"]);
    const p = plan({ scene: s, cursor: [], hover: [], hidden: [lid.node] });
    expect(p.base).toBeNull();
    expect(p.items.map((i) => s.nodes[i.node].instance)).toEqual(["housing"]);
    expect(plan({ scene: s, cursor: [], hover: [] }).base).not.toBeNull();
  });
});

describe("data files (SVG, fonts; Bericht 16)", () => {
  it("kinds", () => {
    expect(isFontFile("fonts/A.TTF")).toBe(true);
    expect(isFontFile("logo.svg")).toBe(false);
    expect(isDataFile("logo.svg") && isDataFile("x.otf")).toBe(true);
    expect(isDataFile("a.scad")).toBe(false);
    expect(isMeshFile("parts/Board.STL") && isDataFile("x.stl") && isBinaryDataFile("x.stl")).toBe(true);
    expect(isBinaryDataFile("logo.svg")).toBe(false);
  });
  it("data URLs round trip, texts stay texts", () => {
    const b = new Uint8Array(70000).map((_, i) => (i * 37) & 255);
    const u = dataUrl(b, "font/ttf");
    expect(u.startsWith("data:font/ttf;base64,")).toBe(true);
    expect(Array.from(fromDataUrl(u)!)).toEqual(Array.from(b));
    expect(fromDataUrl("<svg/>")).toBeNull();
    expect(new TextDecoder().decode(fileBytes("<svg>ä</svg>"))).toBe("<svg>ä</svg>");
    expect(Array.from(fileBytes(u)).length).toBe(70000);
    expect(fileText("a.svg", new TextEncoder().encode("<svg/>"))).toBe("<svg/>");
    expect(fileText("a.ttf", new Uint8Array([1, 2, 3]))).toBe("data:font/ttf;base64,AQID");
    // STL (also binary) as data URL
    expect(fileText("a.stl", new Uint8Array([1, 2, 3]))).toBe("data:model/stl;base64,AQID");
  });
  it("references in a source", () => {
    const src = 'import("../svg/logo.svg", center = true);\ntext("x", font = "fonts/Mine.ttf");\ntext("y", font = "Inter:style=Bold");\nimport("../svg/logo.svg");';
    expect(dataFileRefs(src)).toEqual(["../svg/logo.svg", "fonts/Mine.ttf"]);
    expect(dataFileRefs('import("part.stl", as = "mesh");')).toEqual(["part.stl"]);
  });
});
