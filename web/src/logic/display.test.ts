import { describe, expect, it } from "vitest";
import { plan, planNodes, type Plan } from "./display";
import { apply } from "./math";
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

  it("Cursor auf einer Zone: nur diese Zone blau, Kanten des Blocks zur Prüfung", () => {
    const { f, p } = planAt("fillet-modul", 17, 10);
    expect(modes(f, p)).toEqual(["blue:zone@17"]);
    // The block's edges are not drawn as such; the viewer keeps those in this zone.
    expect(p.edges).toEqual([]);
    expect(p.zoneEdges?.edges.length).toBe(2);
    expect(p.zoneEdges?.select).toBe("inside");
    expect(planNodes(p)).toContain(p.zoneEdges!.zone);
  });

  it("Zone über translate: ohne Umriss und Koordinatenkreuze der Transformation", () => {
    const { f, p } = planAt("fillet-modul", 18, 6);
    expect(modes(f, p)).toEqual(["blue:translate@18"]);
    expect(p.frames).toEqual([]);
  });

  it("select = touch kommt aus op_info", () => {
    const { p } = planAt("fillet-modul", 25, 8);
    expect(p.zoneEdges?.select).toBe("touch");
    expect(p.zoneEdges?.edges.length).toBe(5);
  });

  it("linear_extrude: Profil als Umriss unten (center: auf −h/2)", () => {
    const { f, p } = planAt("profile", 2, 3);
    expect(modes(f, p)).toEqual(["full:linear_extrude@2", "profile:rect@2"]);
    expect(p.items[1].matrix.slice(12, 15)).toEqual([0, 0, -5]);
  });

  it("rotate_extrude: Profil in der xz-Ebene, Drehachse als Linie", () => {
    const { f, p } = planAt("profile", 3, 30);
    expect(modes(f, p)).toEqual(["full:rotate_extrude@3", "profile:translate@3"]);
    // Profile y → z: the matrix maps (x, y, 0) to (40 + x, 0, y).
    const m = p.items[1].matrix;
    const q = apply(m, [10, 12, 0]);
    expect(q[0]).toBeCloseTo(50);
    expect(q[1]).toBeCloseTo(0);
    expect(q[2]).toBeCloseTo(12);
    expect(p.guides).toEqual([expect.objectContaining({ kind: "axis", dashed: true })]);
    const [a, b] = p.guides[0].points;
    expect([a[0], a[1], b[0], b[1]]).toEqual([40, 0, 40, 0]);
    expect(a[2]).toBeLessThan(0);
    expect(b[2]).toBeGreaterThan(12);
  });

  it("loft: alle Profile als Umrisse an ihrer Lage, Ergebnis halbtransparent", () => {
    const { f, p } = planAt("profile", 4, 24);
    expect(modes(f, p)).toEqual(["translucent:loft@4", "profile:circle@5", "profile:rect@6"]);
    expect(p.items[2].matrix.slice(12, 15)).toEqual([0, 40, 20]);
  });

  it("sweep: Pfad als Kurve, Profil am Startpunkt senkrecht zur Tangente", () => {
    const { f, p } = planAt("profile", 8, 30);
    expect(modes(f, p)).toEqual(["full:sweep@8", "profile:circle@8"]);
    const path = p.guides.find((g) => g.kind === "path")!;
    expect(path.points).toEqual([[40, 40, 0], [60, 40, 10], [80, 50, 10]]);
    // Profile normal (z of its matrix) along the start tangent (2, 0, 1)/√5.
    const m = p.items[1].matrix;
    expect(m[8]).toBeCloseTo(2 / Math.sqrt(5));
    expect(m[10]).toBeCloseTo(1 / Math.sqrt(5));
    expect(m.slice(12, 15)).toEqual([40, 40, 0]);
  });

  it("surface und thicken: Kontrollnetz, Fläche als Umriss", () => {
    let r = planAt("profile", 9, 25);
    expect(modes(r.f, r.p)).toEqual(["full:thicken@9", "profile:surface@9"]);
    r = planAt("profile", 9, 50);
    expect(modes(r.f, r.p)).toEqual(["full:surface@9"]);
    expect(r.p.guides.filter((g) => g.kind === "net")).toHaveLength(8);
  });

  it("shell: Zone blau, geöffnete Flächen zur Prüfung", () => {
    const { f, p } = planAt("profile", 11, 30);
    expect(modes(f, p)).toEqual(["full:shell@11", "blue:zone@13"]);
    expect(p.openFaces).toHaveLength(1);
    const q = p.openFaces[0];
    expect(f.scene.nodes[q.body].label).toBe("box");
    expect(q.zones.map((z) => f.scene.nodes[z].label)).toEqual(["zone"]);
    expect(planNodes(p)).toEqual(expect.arrayContaining([q.body, ...q.zones]));
    // Cursor on the zone: only this zone and its faces.
    const z = planAt("profile", 13, 6);
    expect(modes(z.f, z.p)).toEqual(["blue:zone@13"]);
    expect(z.p.openFaces[0].zones).toEqual(q.zones);
  });

  it("Modifier: # rot, % grau, unabhängig vom Cursor", () => {
    const { f, p } = planAt("modifiers", 8, 0);
    expect(modes(f, p)).toEqual(["full:translate@8", "outline:box@8", "red:translate@4", "grey:translate@6"]);
  });

  it("Sperrvolumen (assert_keepout): eigener Modus, ausblendbar", () => {
    const f = fixture("modifiers");
    const scene = structuredClone(f.scene);
    const bg = scene.nodes.find((n) => n.flags.background)!;
    bg.flags.keepout = true;
    const show = plan({ scene, cursor: [], hover: [] });
    expect(show.items.filter((i) => i.node === bg.idx).map((i) => i.mode)).toEqual(["keepout"]);
    expect(show.items.some((i) => i.mode === "grey")).toBe(false);
    const hide = plan({ scene, cursor: [], hover: [], showKeepout: false });
    expect(hide.items.some((i) => i.node === bg.idx)).toBe(false);
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
