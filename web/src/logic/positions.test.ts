import { ChangeSet, EditorState, Text } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { VersionMapper, mapRange } from "./positions";
import { SceneIndex } from "./sceneIndex";
import { fixture, pos } from "./__fixtures__/load";

describe("VersionMapper (alte Quellpositionen)", () => {
  it("bildet Positionen über spätere Änderungen ab", () => {
    const m = new VersionMapper(10);
    m.mark(1, 10);
    // Insert 3 characters at 2, then delete 1 at 0.
    m.apply(ChangeSet.of({ from: 2, insert: "abc" }, 10));
    m.apply(ChangeSet.of({ from: 0, to: 1 }, 13));
    expect(m.map(1, 0)).toBe(0);
    expect(m.map(1, 5)).toBe(7);
    expect(m.map(1, 10)).toBe(12);
  });

  it("führt mehrere Versionen getrennt", () => {
    const m = new VersionMapper(4);
    m.mark(1, 4);
    m.apply(ChangeSet.of({ from: 0, insert: "xx" }, 4));
    m.mark(2, 6);
    m.apply(ChangeSet.of({ from: 0, insert: "y" }, 6));
    expect(m.map(1, 0)).toBe(3);
    expect(m.map(2, 0)).toBe(1);
    m.prune(2);
    expect(m.has(1)).toBe(false);
    expect(m.versions()).toEqual([2]);
  });

  it("begrenzt unbekannte Versionen auf die Dokumentlänge", () => {
    const m = new VersionMapper(5);
    expect(m.map(9, 100)).toBe(5);
    expect(m.mapper(9)(3)).toBe(3);
  });

  it("Bereiche: Einfügen am Anfang verschiebt, am Ende verlängert nicht", () => {
    const m = new VersionMapper(10);
    m.mark(1, 10);
    m.apply(ChangeSet.of([{ from: 2, insert: "--" }, { from: 6, insert: "++" }], 10));
    expect(mapRange(m.mapper(1), 2, 6)).toEqual({ from: 4, to: 8 });
  });

  it("Knoten einer alten Szene bleiben beim Tippen an ihrer Zeile", () => {
    const f = fixture("housing");
    const m = new VersionMapper(f.source.length);
    m.mark(f.scene.version, f.source.length);
    // Two new lines at the top of the document.
    const state = EditorState.create({ doc: f.source });
    const tr = state.update({ changes: { from: 0, insert: "// a\n// b\n" } });
    m.apply(tr.changes);
    const doc: Text = tr.newDoc;
    const idx = new SceneIndex(f.scene, m.mapper(f.scene.version));
    // `translate(k.corner) lid();` was line 22, now 24.
    const sel = idx.select(doc, pos(doc, 24, 2));
    expect(sel.nodes.map((n) => f.scene.nodes[n].label)).toEqual(["translate"]);
    expect(idx.linesOf(doc, sel.nodes)).toEqual([24]);
  });
});
