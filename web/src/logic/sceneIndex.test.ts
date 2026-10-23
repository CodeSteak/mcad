import { describe, expect, it } from "vitest";
import { SceneIndex } from "./sceneIndex";
import { fixture, pos } from "./__fixtures__/load";

describe("Zuordnung Code → Knoten", () => {
  const f = fixture("gehaeuse");
  const idx = new SceneIndex(f.scene);
  const labels = (ns: number[]) => ns.map((n) => f.scene.nodes[n].label);

  it("Cursor in einer Zeile wählt den Knoten der Zeile", () => {
    expect(labels(idx.nodesAt(f.doc, pos(f.doc, 21, 0)).nodes)).toEqual(["gehaeuse"]);
    // In `translate(k.ecke) deckel();` the cursor on `deckel` selects the module call…
    expect(labels(idx.nodesAt(f.doc, pos(f.doc, 22, 20)).nodes)).toEqual(["deckel"]);
    // … elsewhere on the line the outermost statement.
    expect(labels(idx.nodesAt(f.doc, pos(f.doc, 22, 2)).nodes)).toEqual(["translate"]);
    expect(labels(idx.nodesAt(f.doc, pos(f.doc, 22, 999)).nodes)).toEqual(["translate"]);
  });

  it("Zeilen ohne Geometrie wählen nichts", () => {
    expect(idx.nodesAt(f.doc, pos(f.doc, 20, 0)).nodes).toEqual([]);
    expect(idx.nodesAt(f.doc, pos(f.doc, 1, 3)).nodes).toEqual([]);
  });

  it("eine Zeile mit mehreren Instanzen wählt alle", () => {
    // module deckel() cube(…) (line 19) is instantiated twice.
    const sel = idx.nodesAt(f.doc, pos(f.doc, 19, 18));
    expect(labels(sel.nodes)).toEqual(["box", "box"]);
  });

  it("Referenzen der Zeile", () => {
    const sel = idx.select(f.doc, pos(f.doc, 23, 0));
    expect(sel.refs.map((r) => f.scene.refs[r].target.kind)).toEqual(["anchor", "anchor"]);
    expect(idx.refAt(pos(f.doc, 22, 12))).toBe(0);
    expect(idx.refAt(pos(f.doc, 22, 25))).toBeNull();
  });

  it("Modul unter dem Cursor", () => {
    expect(idx.moduleAt(pos(f.doc, 11, 4))).toBe("gehaeuse");
    expect(idx.moduleAt(pos(f.doc, 19, 20))).toBe("deckel");
    expect(idx.moduleAt(pos(f.doc, 21, 0))).toBeNull();
  });

  it("Rasterdeklaration auf der Zeile", () => {
    expect(idx.gridsOnLine(f.doc, 25)).toEqual([1]);
    expect(idx.gridsOnLine(f.doc, 22)).toEqual([]);
  });
});

describe("Zuordnung Ansicht → Code", () => {
  const f = fixture("gehaeuse");
  const idx = new SceneIndex(f.scene);
  const tagOf = (label: string, line: number) => f.scene.nodes.find((n) => n.label === label && n.range.line === line)!.tag;

  it("Fläche eines Untermoduls führt zur Aufrufzeile im Kontext", () => {
    // Faces of the boxes inside gehaeuse (module instance 1) → node of `k: gehaeuse(…)`.
    const nodes = idx.nodesForTag(tagOf("box", 11));
    expect(nodes.map((n) => f.scene.nodes[n].range.line)).toEqual([21]);
    expect(idx.linesOf(f.doc, nodes)).toEqual([21]);
    expect(idx.instanceOfNode(nodes[0])).toBe(1);
  });

  it("mehrdeutige Tags werden über die Lage getrennt, sonst alle", () => {
    const tag = tagOf("box", 19);
    expect(idx.linesOf(f.doc, idx.nodesForTag(tag))).toEqual([22, 23]);
    // A point on the lid at k.ecke = [60, 30, 20].
    expect(idx.linesOf(f.doc, idx.nodesForTag(tag, [65, 35, 22]))).toEqual([22]);
    expect(idx.linesOf(f.doc, idx.nodesForTag(tag, [5, 5, 22]))).toEqual([23]);
  });

  it("Kontext Modul: Flächen gehören zu den Zeilen im Modul", () => {
    const m = fixture("gehaeuse-modul");
    const mi = new SceneIndex(m.scene);
    const inner = m.scene.nodes.find((n) => n.label === "box" && n.range.line === 12)!;
    expect(mi.linesOf(m.doc, mi.nodesForTag(inner.tag))).toEqual([12]);
  });
});
