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

const EX = join(__dirname, "../../../tests/models/projekt-gehaeuse");
const read = (p: string) => readFileSync(join(EX, p), "utf8");

describe("Pfade wie die Engine", () => {
  it("relativ zur einbindenden Datei", () => {
    expect(resolvePath("main.scad", "lib/a.scad")).toBe("lib/a.scad");
    expect(resolvePath("lib/a.scad", "b.scad")).toBe("lib/b.scad");
    expect(resolvePath("lib/a.scad", "../konst.scad")).toBe("konst.scad");
    expect(resolvePath("lib/a.scad", "./x/../y.scad")).toBe("lib/y.scad");
    expect(resolvePath("main.scad", "../aussen.scad")).toBe("../aussen.scad");
    expect(resolvePath("a\\b.scad", "c\\d.scad")).toBe("a/c/d.scad");
    expect(normalizePath("/a/./b/../c")).toBe("/a/c");
  });

  it("include/use im Text, nicht in Kommentaren", () => {
    const t = "// include <nein.scad>\ninclude <konst.scad>\n/* use <x> */ use <lib/teile.mcad>;";
    expect(includeStatements(t).map((s) => [s.path, s.isUse])).toEqual([
      ["konst.scad", false],
      ["lib/teile.mcad", true],
    ]);
    const at = t.indexOf("konst");
    expect(includeAt(t, at)?.path).toBe("konst.scad");
    expect(includeAt(t, 3)).toBeNull();
  });

  it("Hauptdatei eines Ordners", () => {
    const files = { "gehaeuse.mcad": read("gehaeuse.mcad"), "konstanten.mcad": read("konstanten.mcad"), "lib/teile.mcad": read("lib/teile.mcad") };
    expect(guessMain(files)).toBe("gehaeuse.mcad");
    expect(guessMain({ "b.scad": "cube(1);", "a.scad": "include <b.scad>" })).toBe("a.scad");
    expect(guessMain({ "x.txt": "" })).toBeNull();
    // .mcad and .scad mixed in one project
    expect(guessMain({ "lib/a.scad": "module a() {}", "teil.mcad": "use <lib/a.scad>\na();", "notiz.txt": "" })).toBe("teil.mcad");
    expect(guessMain({ "alt.SCAD": "use <neu.mcad>", "neu.mcad": "module n() {}" })).toBe("alt.SCAD");
  });

  it("Modelldateien: .mcad neu, .scad weiterhin", () => {
    expect(["a.mcad", "a.scad", "lib/B.MCAD", "c.Scad"].every(isModelFile)).toBe(true);
    expect(["a.svg", "a.mcad.txt", "scad", "a.cad"].some(isModelFile)).toBe(false);
    expect(isProjectFile("x/y.scad") && isProjectFile("x/y.mcad") && isProjectFile("logo.svg")).toBe(true);
    expect(withModelExt("lib/teile")).toBe("lib/teile.mcad");
    expect(withModelExt("alt.scad")).toBe("alt.scad");
    expect(stripModelExt("teil.mcad")).toBe("teil");
    expect(stripModelExt("teil.scad")).toBe("teil");
    expect(stripCommonFolder({ "p/a.scad": "1", "p/lib/b.scad": "2" })).toEqual({ files: { "a.scad": "1", "lib/b.scad": "2" }, folder: "p" });
    expect(stripCommonFolder({ "a.scad": "1", "p/b.scad": "2" }).folder).toBeNull();
  });
});

describe("Szene eines Projekts", () => {
  const f = fixture("projekt");
  const s = f.scene;

  it("Dateien, Bauteile, include-Anweisungen", () => {
    expect(s.files.map((x) => [x.id, x.path, x.role])).toEqual([
      [0, "gehaeuse.mcad", "main"],
      [1, "konstanten.mcad", "include"],
      [2, "lib/teile.mcad", "use"],
    ]);
    expect(s.parts.map((p) => p.name)).toEqual(["gehaeuse", "deckel"]);
    expect(s.includes.map((i) => [i.target, i.is_use, i.found])).toEqual([
      ["konstanten.mcad", false, true],
      ["lib/teile.mcad", true, true],
    ]);
    expect(fileIds(s, "lib/teile.mcad", "gehaeuse.mcad")).toEqual(new Set([2]));
    expect(fileIds(s, "gehaeuse.mcad", "gehaeuse.mcad")).toEqual(new Set([0]));
  });

  it("Index der Hauptdatei: Bereiche anderer Dateien zählen nicht", () => {
    const idx = new SceneIndex(s, undefined, fileIds(s, "gehaeuse.mcad", "gehaeuse.mcad"));
    const hint = s.hints.find((h) => h.text === "60")!;
    expect(hint.range.file).toBe(1);
    expect(isOff(idx.range(hint.range))).toBe(true);
    const call = s.nodes.find((n) => n.instance === "gehaeuse")!;
    expect(idx.linesOf(f.doc, [call.idx])).toEqual([6]);
    // A node of the library maps to no line of the main file.
    const libNode = s.nodes.find((n) => n.range.file === 2)!;
    expect(idx.linesOf(f.doc, [libNode.idx])).toEqual([]);
  });

  it("Index der Bibliothek: Module und Zeilen in lib/teile.mcad", () => {
    const lib = read("lib/teile.mcad");
    const doc = Text.of(lib.split("\n"));
    const m = fixture("projekt-modul").scene;
    const idx = new SceneIndex(m, undefined, fileIds(m, "lib/teile.mcad", "gehaeuse.mcad"));
    const pos = doc.line(17).from + 6;
    expect(idx.moduleAt(pos)).toBe("gehaeuse");
    const sel = idx.select(doc, doc.line(21).from + 10);
    expect(sel.nodes.length).toBeGreaterThan(0);
    expect(rangePath(m, m.nodes[sel.nodes[0]].range, "gehaeuse.mcad")).toBe("lib/teile.mcad");
  });

  it("Dekorationen nur für die Datei im Editor", () => {
    const konst = read("konstanten.mcad");
    const doc = Text.of(konst.split("\n"));
    const base = { complete: null, diagnostics: { version: 0, items: [] }, mapper: new VersionMapper(doc.length), engineError: null, viewHoverLines: [], hoverLine: null, hoverRef: null };
    const inKonst = collectLines(doc, { ...base, scene: s, activeFile: "konstanten.mcad", mainFile: "gehaeuse.mcad" }, []);
    expect(inKonst.lines.get(2)?.segs).toEqual([{ kind: "hint", text: "= 60" }]);
    // The same scene seen from the main file: no hints of konstanten.mcad on its lines.
    const main = Text.of(f.source.split("\n"));
    const inMain = collectLines(main, { ...base, scene: s, activeFile: "gehaeuse.mcad", mainFile: "gehaeuse.mcad" }, []);
    expect([...inMain.lines.values()].flatMap((l) => l.segs).filter((x) => x.kind === "hint")).toEqual([]);
    // Diagnostics name their file.
    const d = { severity: "error" as const, message: "x", source: "eval" as const, node: null, file: "lib/teile.mcad", range: { from: 0, to: 1, line: 1, col: 1, end_line: 1, end_col: 2 } };
    const lines = collectLines(main, { ...base, scene: null, diagnostics: { version: 0, items: [d] }, activeFile: "gehaeuse.mcad", mainFile: "gehaeuse.mcad" }, []).lines;
    expect(lines.size).toBe(0);
  });

  it("ausgeblendete Bauteile: Modell in Teilen ohne sie", () => {
    const deckel = s.parts.find((p) => p.name === "deckel")!;
    const pieces = visiblePieces(s, [deckel.node]);
    const labels = pieces.map((n) => n.instance ?? n.label);
    expect(labels).toEqual(["gehaeuse"]);
    const p = plan({ scene: s, cursor: [], hover: [], hidden: [deckel.node] });
    expect(p.base).toBeNull();
    expect(p.items.map((i) => s.nodes[i.node].instance)).toEqual(["gehaeuse"]);
    expect(plan({ scene: s, cursor: [], hover: [] }).base).not.toBeNull();
  });
});

describe("data files (SVG, fonts; Bericht 16)", () => {
  it("kinds", () => {
    expect(isFontFile("schriften/A.TTF")).toBe(true);
    expect(isFontFile("logo.svg")).toBe(false);
    expect(isDataFile("logo.svg") && isDataFile("x.otf")).toBe(true);
    expect(isDataFile("a.scad")).toBe(false);
    expect(isMeshFile("teile/Platine.STL") && isDataFile("x.stl") && isBinaryDataFile("x.stl")).toBe(true);
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
    const src = 'import("../svg/logo.svg", center = true);\ntext("x", font = "fonts/Meine.ttf");\ntext("y", font = "Inter:style=Bold");\nimport("../svg/logo.svg");';
    expect(dataFileRefs(src)).toEqual(["../svg/logo.svg", "fonts/Meine.ttf"]);
    expect(dataFileRefs('import("teil.stl", as = "mesh");')).toEqual(["teil.stl"]);
  });
});
