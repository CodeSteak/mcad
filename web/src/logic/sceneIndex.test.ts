import { describe, expect, it } from "vitest";
import { SceneIndex } from "./sceneIndex";
import { fixture, pos } from "./__fixtures__/load";

describe("Mapping code → nodes", () => {
  const f = fixture("housing");
  const idx = new SceneIndex(f.scene);
  const labels = (ns: number[]) => ns.map((n) => f.scene.nodes[n].label);

  it("the cursor in a line selects the line's node", () => {
    expect(labels(idx.nodesAt(f.doc, pos(f.doc, 21, 0)).nodes)).toEqual(["housing"]);
    // In `translate(k.corner) lid();` the cursor on `lid` selects the module call…
    expect(labels(idx.nodesAt(f.doc, pos(f.doc, 22, 21)).nodes)).toEqual(["lid"]);
    // … elsewhere on the line the outermost statement.
    expect(labels(idx.nodesAt(f.doc, pos(f.doc, 22, 2)).nodes)).toEqual(["translate"]);
    expect(labels(idx.nodesAt(f.doc, pos(f.doc, 22, 999)).nodes)).toEqual(["translate"]);
  });

  it("lines without geometry select nothing", () => {
    expect(idx.nodesAt(f.doc, pos(f.doc, 20, 0)).nodes).toEqual([]);
    expect(idx.nodesAt(f.doc, pos(f.doc, 1, 3)).nodes).toEqual([]);
  });

  it("a line with several instances selects all", () => {
    // module lid() cube(…) (line 19) is instantiated twice.
    const sel = idx.nodesAt(f.doc, pos(f.doc, 19, 15));
    expect(labels(sel.nodes)).toEqual(["box", "box"]);
  });

  it("references of the line", () => {
    const sel = idx.select(f.doc, pos(f.doc, 23, 0));
    expect(sel.refs.map((r) => f.scene.refs[r].target.kind)).toEqual(["anchor", "anchor"]);
    expect(idx.refAt(pos(f.doc, 22, 12))).toBe(0);
    expect(idx.refAt(pos(f.doc, 22, 24))).toBeNull();
  });

  it("module under the cursor", () => {
    expect(idx.moduleAt(pos(f.doc, 11, 4))).toBe("housing");
    expect(idx.moduleAt(pos(f.doc, 19, 17))).toBe("lid");
    expect(idx.moduleAt(pos(f.doc, 21, 0))).toBeNull();
  });

  it("grid declaration on the line", () => {
    expect(idx.gridsOnLine(f.doc, 25)).toEqual([1]);
    expect(idx.gridsOnLine(f.doc, 22)).toEqual([]);
  });
});

describe("Mapping view → code", () => {
  const f = fixture("housing");
  const idx = new SceneIndex(f.scene);
  const tagOf = (label: string, line: number) => f.scene.nodes.find((n) => n.label === label && n.range.line === line)!.tag;

  it("a face of a submodule leads to the call line in the context", () => {
    // Faces of the boxes inside housing (module instance 1) → node of `k: housing(…)`.
    const nodes = idx.nodesForTag(tagOf("box", 11));
    expect(nodes.map((n) => f.scene.nodes[n].range.line)).toEqual([21]);
    expect(idx.linesOf(f.doc, nodes)).toEqual([21]);
    expect(idx.instanceOfNode(nodes[0])).toBe(1);
  });

  it("ambiguous tags are separated by position, otherwise all", () => {
    const tag = tagOf("box", 19);
    expect(idx.linesOf(f.doc, idx.nodesForTag(tag))).toEqual([22, 23]);
    // A point on the lid at k.corner = [60, 30, 20].
    expect(idx.linesOf(f.doc, idx.nodesForTag(tag, [65, 35, 22]))).toEqual([22]);
    expect(idx.linesOf(f.doc, idx.nodesForTag(tag, [5, 5, 22]))).toEqual([23]);
  });

  it("module context: faces belong to the lines in the module", () => {
    const m = fixture("housing-module");
    const mi = new SceneIndex(m.scene);
    const inner = m.scene.nodes.find((n) => n.label === "box" && n.range.line === 12)!;
    expect(mi.linesOf(m.doc, mi.nodesForTag(inner.tag))).toEqual([12]);
  });
});
