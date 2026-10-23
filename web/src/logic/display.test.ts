import { describe, expect, it } from "vitest";
import { plan, type Plan } from "./display";
import { SceneIndex } from "./sceneIndex";
import { fixture, pos } from "./__fixtures__/load";

function modes(f: ReturnType<typeof fixture>, p: Plan): string[] {
  return p.items.map((i) => `${i.mode}:${f.scene.nodes[i.node].label}@${f.scene.nodes[i.node].range.line}`);
}

function planAt(name: Parameters<typeof fixture>[0], line: number, col: number, hoverLine?: number) {
  const f = fixture(name);
  const idx = new SceneIndex(f.scene);
  const cursor = idx.nodesAt(f.doc, pos(f.doc, line, col)).nodes;
  const hover = hoverLine ? idx.nodesAt(f.doc, pos(f.doc, hoverLine, 8)).nodes : [];
  return { f, p: plan({ scene: f.scene, cursor, hover }) };
}

describe("Darstellungszustände", () => {
  it("ohne Auswahl: Modell voll, Anker benannter Instanzen", () => {
    const f = fixture("gehaeuse");
    const p = plan({ scene: f.scene, cursor: [], hover: [] });
    expect(p.base?.mode).toBe("full");
    expect(p.items).toEqual([]);
    expect(p.anchors.map((a) => a.anchor.text)).toEqual(["k.ecke", "k.oben"]);
    expect(p.status).toBeNull();
  });

  it("Cursor auf Zeile mit Geometrie: Knoten voll, Rest gedimmt", () => {
    const { f, p } = planAt("gehaeuse", 21, 0);
    expect(p.base?.mode).toBe("dim");
    expect(modes(f, p)).toEqual(["full:gehaeuse@21"]);
    // Anchors of the selected instance are active.
    expect(p.anchors.filter((a) => a.active)).toHaveLength(2);
  });

  it("Cursor auf Zeile ohne Geometrie dimmt nicht", () => {
    const { p } = planAt("gehaeuse", 26, 4);
    expect(p.base?.mode).toBe("full");
  });

  it("Transformation: Kreuz vorher gestrichelt, nachher durchgezogen, Umriss am Ursprung", () => {
    const { f, p } = planAt("gehaeuse", 22, 2);
    expect(modes(f, p)).toEqual(["full:translate@22", "outline:deckel@22"]);
    expect(p.frames.map((x) => x.dashed)).toEqual([true, false]);
    expect(p.frames[0].matrix.slice(12, 15)).toEqual([0, 0, 0]);
    expect(p.frames[1].matrix.slice(12, 15)).toEqual([60, 30, 20]);
    // The outline sits at the source position (system before the translation).
    expect(p.items[1].matrix.slice(12, 15)).toEqual([0, 0, 0]);
  });

  it("difference: abgezogene Körper rot", () => {
    const { f, p } = planAt("gehaeuse-modul", 10, 6);
    expect(modes(f, p)).toEqual(["full:difference@10", "red:box@12"]);
  });

  it("Hover über Zeile: halbtransparent, auch wenn abgezogen", () => {
    const { f, p } = planAt("gehaeuse-modul", 11, 6, 12);
    expect(modes(f, p)).toEqual(["full:box@11", "hover:box@12"]);
  });

  it("fillet: Zonen blau, ausgewählte Kanten", () => {
    const { f, p } = planAt("fillet-modul", 12, 10);
    expect(modes(f, p)).toEqual(["full:fillet@12", "blue:zone@17", "blue:translate@18"]);
    expect(p.edges.length).toBeGreaterThan(0);
    expect(p.edges.every((e) => e.kind === "selected")).toBe(true);
  });

  it("Cursor auf einer Zone: nur diese Zone blau", () => {
    const { f, p } = planAt("fillet-modul", 17, 10);
    expect(modes(f, p)).toEqual(["blue:zone@17"]);
  });

  it("Modifier: # rot, % grau, unabhängig vom Cursor", () => {
    const { f, p } = planAt("modifiers", 8, 0);
    expect(modes(f, p)).toEqual(["full:translate@8", "outline:box@8", "red:translate@4", "grey:translate@6"]);
  });

  it("! : Hinweis für die Statuszeile", () => {
    const f = fixture("root-modifier");
    const p = plan({ scene: f.scene, cursor: [], hover: [] });
    expect(p.status).toContain("Zeile 3");
    expect(p.base?.node).toBe(f.scene.root);
  });

  it("intersection: Kinder halbtransparent, Ergebnis voll", () => {
    const { f, p } = planAt("nested", 3, 8);
    expect(modes(f, p)).toEqual(["full:intersection@3", "ghost:box@4", "ghost:sphere@5"]);
  });

  it("Hover über union: Kinder mit dünnem Umriss", () => {
    const { f, p } = planAt("nested", 20, 0, 7);
    expect(modes(f, p).filter((m) => m.startsWith("outline"))).toEqual(["outline:cylinder@8", "outline:rotate@9", "outline:rotate@10"]);
  });

  it("2D-Formen: Umriss mit leichter Füllung (flach)", () => {
    const { f, p } = planAt("spacers-2d", 6, 8);
    expect(p.items.length).toBe(4);
    expect(p.items.every((i) => i.flat)).toBe(true);
    expect(modes(f, p)).toEqual(["full:circle@6", "full:circle@6", "full:circle@6", "full:circle@6"]);
    // The difference of the profile: subtracted circle red, also flat.
    const d = planAt("spacers-2d", 5, 60);
    expect(d.p.items.filter((i) => i.mode === "red").every((i) => i.flat)).toBe(true);
  });

  it("Maus in der Ansicht: Knoten mit Kanten in Akzentfarbe", () => {
    const f = fixture("gehaeuse");
    const p = plan({ scene: f.scene, cursor: [], hover: [], picked: [1] });
    expect(modes(f, p)).toEqual(["picked:gehaeuse@21"]);
  });

  it("Hover über Ankerreferenz hebt den Anker hervor", () => {
    const f = fixture("winkel");
    const p = plan({ scene: f.scene, cursor: [], hover: [], hoverAnchor: { inst: 1, anchor: 0 } });
    expect(p.anchors).toHaveLength(6);
    expect(p.anchors.filter((a) => a.active).map((a) => a.anchor.text)).toEqual(["a.boden_loch"]);
  });
});
