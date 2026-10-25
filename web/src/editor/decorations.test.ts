import { ChangeSet, Text } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import type { Diagnostic } from "../engine/protocol";
import { VersionMapper } from "../logic/positions";
import { fixture } from "../logic/__fixtures__/load";
import { collectLines, type DecorationInput } from "./decorations";
import { disabledStatements, scadParser } from "./lang";

function input(over: Partial<DecorationInput>, mapper = new VersionMapper(1000)): DecorationInput {
  return {
    scene: null,
    complete: null,
    diagnostics: { version: 0, items: [] },
    mapper,
    engineError: null,
    viewHoverLines: [],
    hoverLine: null,
    hoverRef: null,
    ...over,
  };
}

const diag = (severity: Diagnostic["severity"], from: number, to: number, message: string, line = 1): Diagnostic => ({
  severity,
  message,
  source: "syntax",
  node: null,
  range: { from, to, line, col: 1, end_line: line, end_col: 1 },
});

describe("Editor-Dekorationen", () => {
  it("Syntaxfehler: Stelle unterstrichen, Zeile rot, Meldung am Ende", () => {
    const doc = Text.of(["cube(10);", "sphere(;"]);
    const { lines, marks } = collectLines(doc, input({ diagnostics: { version: 0, items: [diag("error", 17, 17, "Ausdruck erwartet", 2)] } }), []);
    expect([...lines.get(2)!.classes]).toContain("cm-l-error");
    expect(lines.get(2)!.segs).toEqual([{ kind: "error", text: "Ausdruck erwartet" }]);
    // Empty range → one character marked.
    expect(marks).toEqual([{ from: 17, to: 18, cls: "cm-d-error" }]);
  });

  it("Warnung: Zeile gelb", () => {
    const doc = Text.of(["a", "b"]);
    const { lines } = collectLines(doc, input({ diagnostics: { version: 0, items: [{ ...diag("warning", 2, 3, "0 Kanten", 2), source: "kernel" }] } }), []);
    expect([...lines.get(2)!.classes]).toEqual(["cm-l-warning"]);
  });

  it("Diagnosen einer alten Version folgen den Änderungen", () => {
    const doc0 = Text.of(["cube(10);", "sphere(;"]);
    const m = new VersionMapper(doc0.length);
    m.mark(3, doc0.length);
    const ch = ChangeSet.of({ from: 0, insert: "// neu\n" }, doc0.length);
    m.apply(ch);
    const doc = ch.apply(doc0);
    const { lines } = collectLines(doc, input({ diagnostics: { version: 3, items: [diag("error", 17, 17, "x", 2)] } }, m), []);
    expect(lines.get(3)?.classes.has("cm-l-error")).toBe(true);
    expect(lines.get(2)?.classes.has("cm-l-error") ?? false).toBe(false);
  });

  it("Inline-Hinweise: Werte und Kantenanzahl", () => {
    const f = fixture("fillet-modul");
    const { lines } = collectLines(f.doc, input({ scene: f.scene, complete: f.scene }, new VersionMapper(f.source.length)), []);
    expect(lines.get(9)!.segs).toEqual([{ kind: "hint", text: "= [15, 10, 0]" }]);
    expect(lines.get(12)!.segs).toEqual([{ kind: "hint", text: "2 Zonen · 2 Kanten" }]);
    expect(lines.get(11)!.segs).toEqual([{ kind: "hint", text: "1 Zone · 5 Kanten" }]);
  });

  it("*-Anweisungen ausgegraut, Hover-Zeilen markiert", () => {
    const f = fixture("modifiers");
    const dis = disabledStatements(scadParser.parse(f.source), f.source);
    const { lines } = collectLines(f.doc, input({ complete: f.scene, viewHoverLines: [3], hoverLine: 8 }, new VersionMapper(f.source.length)), dis);
    expect(lines.get(7)!.classes.has("cm-l-disabled")).toBe(true);
    expect(lines.get(8)!.classes.has("cm-l-disabled")).toBe(false);
    expect(lines.get(3)!.classes.has("cm-l-viewhover")).toBe(true);
    expect(lines.get(8)!.classes.has("cm-l-hover")).toBe(true);
  });

  it("Watchdog-Fehler an einem Knoten färbt dessen Zeile", () => {
    const f = fixture("gehaeuse");
    const node = f.scene.nodes.find((n) => n.range.line === 22)!.idx;
    const { lines } = collectLines(f.doc, input({ complete: f.scene, engineError: { node, version: 1, message: "Engine hängt" } }, new VersionMapper(f.source.length)), []);
    expect(lines.get(22)!.classes.has("cm-l-error")).toBe(true);
    expect(lines.get(22)!.segs[0]).toEqual({ kind: "error", text: "Engine hängt" });
  });

  it("Inline-Schalter nur für Bool-Literale bei Zuweisungen", () => {
    const g = fixture("griffe");
    const { controls } = collectLines(g.doc, input({ scene: g.scene }, new VersionMapper(g.source.length)), []);
    // Number literals (line 12) have no inline slider.
    expect([...controls.keys()]).toEqual([13]);
    const b = controls.get(13)!;
    expect(b).toMatchObject({ kind: "bool", value: true });
    expect(g.doc.sliceString(b.from, b.to)).toBe("true");
    // Vectors have no inline control (they have a handle).
    expect(controls.has(2)).toBe(false);
  });
});
