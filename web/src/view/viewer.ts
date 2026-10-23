// 3D view (three.js): draws the display plan of the shown scene, grids and anchors; orbit,
// pan, zoom, standard views; picking (face → tag → node → editor line).

import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { LineSegments2 } from "three/addons/lines/LineSegments2.js";
import { LineSegmentsGeometry } from "three/addons/lines/LineSegmentsGeometry.js";
import type { EngineClient } from "../engine/client";
import type { Mat4, Scene, Vec3 } from "../engine/protocol";
import { definitionTarget } from "../logic/context";
import { type Plan, plan as makePlan, planNodes } from "../logic/display";
import { type GridOverlay, gridOverlay, modelHull, viewBox } from "../logic/grids";
import { type HandleGeom, type Ray, handleGeom, handleLabel, handlesForCursor } from "../logic/handles";
import { type Box3, boxDiagonal, emptyBox, isEmpty, length, sub } from "../logic/math";
import { MeshVolume, edgesInZone, facesInZones } from "../logic/zones";
import { needsRefit } from "../logic/fit";
import type { DragController, DragPreview, DragView } from "../app/drag";
import { buildHandleLayer, readHandleColors, type HandleColors } from "./handleLayer";
import { EMPTY_SELECTION, type CursorSelection, isOff } from "../logic/sceneIndex";
import { rangePath } from "../logic/files";
import { bridge } from "../state/bridge";
import { project } from "../state/project";
import { type CadState, store } from "../state/store";
import { Materials, readPalette } from "./materials";
import { type CapBody, SectionLayer } from "./sectionLayer";
import { type SectionState, effectiveNormal, keeps, offset, snapCandidates, withOffset } from "../logic/section";
import type { MeshCache } from "./meshCache";

export type StandardView = "iso" | "top" | "front" | "right";

const VIEW_DIRS: Record<StandardView, { dir: THREE.Vector3; up: THREE.Vector3 }> = {
  iso: { dir: new THREE.Vector3(1, -1.25, 0.9).normalize(), up: new THREE.Vector3(0, 0, 1) },
  top: { dir: new THREE.Vector3(0, -1e-4, 1).normalize(), up: new THREE.Vector3(0, 0, 1) },
  front: { dir: new THREE.Vector3(0, -1, 0), up: new THREE.Vector3(0, 0, 1) },
  right: { dir: new THREE.Vector3(1, 0, 0), up: new THREE.Vector3(0, 0, 1) },
};

function m4(m: Mat4): THREE.Matrix4 {
  return new THREE.Matrix4().fromArray(m);
}

export interface PickResult {
  nodes: number[];
  lines: number[];
  point: Vec3;
  tag: number;
  scene: Scene;
  /** The face comes from another project file than the editor's: its file and line. */
  other: { path: string; line: number; pos: number } | null;
}

export interface ViewState {
  sceneVersion: number | null;
  complete: boolean;
  context: string | null;
  selection: CursorSelection;
  hoverNodes: number[];
  plan: {
    base: string | null;
    items: Array<{ node: number; mode: string; label: string; line: number; flat: boolean; why: string }>;
    edges: number;
    frames: number;
    anchors: number;
    activeAnchors: number;
    status: string | null;
    /** Guide lines by kind (axis, path, net). */
    guides: Record<string, number>;
    /** Polyline points of the guides (context coordinates). */
    guidePoints: Array<{ kind: string; points: Vec3[] }>;
  };
  /** Drawn edge lines by kind (selected, error; zone edges are "selected"). */
  edgeKinds: Record<string, number>;
  /** Cursor on a zone: selected edges of the block in this zone / of the block. */
  zoneEdges: { zone: number; inZone: number; total: number } | null;
  /** shell: faces of the body in the zones (drawn coloured) and the engine's count. */
  openFaces: Array<{ shell: number; faces: number; engine: number }>;
  camera: { autoFit: boolean; refitHint: boolean; distance: number; target: Vec3 };
  grid: { lines: number; dashed: number; points: number; highlight: string[] };
  missingMeshes: number;
  objects: number;
}

/** A handle as drawn (test hook, report). */
export interface HandleView {
  id: number;
  kind: string;
  /** Axis of angle, plane and scale handles. */
  axis: number | null;
  index: number;
  style: string;
  line: number;
  value: number[];
  screen: { x: number; y: number } | null;
  label: string;
}

/** Grip pick radius in pixels. */
const GRIP_PX = 10;

export class Viewer implements DragView {
  readonly renderer: THREE.WebGLRenderer;
  readonly camera: THREE.PerspectiveCamera;
  readonly controls: OrbitControls;
  private scene3 = new THREE.Scene();
  private content = new THREE.Group();
  private overlay = new THREE.Group();
  private helpers = new THREE.Group();
  private handleGroup = new THREE.Group();
  private handleColors: HandleColors;
  private preview: DragPreview | null = null;
  private snapLabel: HTMLDivElement;
  /** Drag controller (set by the runtime). */
  drag: DragController | null = null;
  /** Handles drawn for the cursor line. */
  lastHandles: HandleGeom[] = [];
  private gizmo: { scene: THREE.Scene; camera: THREE.OrthographicCamera; group: THREE.Group };
  private materials: Materials;
  /** Section view (clipping plane, caps, contour, grip). */
  readonly section: SectionLayer;
  /** Opaque solids drawn in this build (they get section caps). */
  private capBodies: CapBody[] = [];
  private sectionDrag: { t0: number; center: THREE.Vector3; n: THREE.Vector3; frames: number; start: number; last: number; maxGap: number } | null = null;
  private pickables: THREE.Mesh[] = [];
  private raycaster = new THREE.Raycaster();
  private renderQueued = false;
  private buildQueued = false;
  private unsub: () => void;
  private resizeObs: ResizeObserver;
  private lastFitKey = "";
  /** The camera still shows the last automatic fit (refit when the viewport size changes). */
  private autoFit = false;
  /** Model box of the last fit (refit after a clear size change). */
  private fitBox: Box3 | null = null;
  /** Zone volumes by mesh key and placement (built on demand). */
  private volumes = new Map<string, MeshVolume>();
  /** Results of the zone tests (hover and cursor changes rebuild often). */
  private zoneCache = new Map<string, unknown>();
  lastZoneEdges: ViewState["zoneEdges"] = null;
  lastOpenFaces: ViewState["openFaces"] = [];
  lastEdgeKinds: Record<string, number> = {};
  private lastShown: Scene | null = null;
  private pickedScene: Scene | null = null;
  private drawnVersions = new Set<string>();
  private down: { x: number; y: number; t: number } | null = null;
  private hoverQueued: PointerEvent | null = null;
  /** Last mouse position over the canvas (hover is re-picked when the scene changes). */
  private pointer: { x: number; y: number } | null = null;
  private disposed = false;
  /** Number of rendered frames. */
  frames = 0;
  lastPlan: Plan | null = null;
  lastGrid: GridOverlay | null = null;
  lastSelection: CursorSelection = EMPTY_SELECTION;
  lastHoverNodes: number[] = [];
  missingMeshes = 0;
  /** Mode the base was drawn with (full while the selected node's mesh is loading). */
  baseMode: string | null = null;
  modelBox: Box3 = emptyBox();

  constructor(
    readonly container: HTMLElement,
    private client: EngineClient,
    private meshes: MeshCache,
  ) {
    this.renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true, stencil: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    // Section view: model materials get a clipping plane (local clipping).
    this.renderer.localClippingEnabled = true;
    container.appendChild(this.renderer.domElement);
    this.renderer.domElement.setAttribute("data-testid", "viewport-canvas");
    this.camera = new THREE.PerspectiveCamera(35, 1, 0.1, 10000);
    this.camera.up.set(0, 0, 1);
    this.camera.position.set(80, -100, 70);
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = false;
    this.controls.screenSpacePanning = true;
    this.controls.zoomToCursor = true;
    this.controls.addEventListener("change", () => this.requestRender());
    this.controls.addEventListener("start", () => (this.autoFit = false));

    this.scene3.add(this.helpers, this.content, this.overlay, this.handleGroup);
    {
      const pal = readPalette();
      this.section = new SectionLayer(pal.cap, pal.edge, pal.accent);
      this.scene3.add(this.section.group);
    }
    this.handleColors = readHandleColors();
    this.snapLabel = document.createElement("div");
    this.snapLabel.className = "snap-label";
    this.snapLabel.setAttribute("data-testid", "snap-label");
    this.snapLabel.hidden = true;
    container.appendChild(this.snapLabel);
    const hemi = new THREE.HemisphereLight(0xffffff, 0x8a8f98, 1.6);
    hemi.position.set(0, 0, 1);
    this.scene3.add(hemi);
    const head = new THREE.DirectionalLight(0xffffff, 1.4);
    head.position.set(0.3, 0.2, 1);
    this.camera.add(head);
    this.scene3.add(this.camera);

    this.materials = new Materials(readPalette());
    this.gizmo = this.makeGizmo();
    this.applyTheme();

    this.resizeObs = new ResizeObserver(() => this.resize());
    this.resizeObs.observe(container);
    this.resize();

    const el = this.renderer.domElement;
    // Grips win over orbiting: the capture listener on the container runs before OrbitControls.
    container.addEventListener("pointerdown", (e) => this.onGripDown(e), true);
    el.addEventListener("pointermove", (e) => this.onPointerMove(e));
    el.addEventListener("pointerleave", () => {
      this.pointer = null;
      this.setViewHover(null);
    });
    el.addEventListener("pointerdown", (e) => (this.down = { x: e.clientX, y: e.clientY, t: performance.now() }));
    el.addEventListener("pointerup", (e) => this.onPointerUp(e));
    el.addEventListener("dblclick", (e) => this.onDoubleClick(e));

    this.unsub = store.subscribe((s, p) => this.onStore(s, p));
    this.requestBuild();
  }

  dispose(): void {
    this.disposed = true;
    this.unsub();
    this.resizeObs.disconnect();
    this.controls.dispose();
    this.clearGroup(this.content);
    this.clearGroup(this.overlay);
    this.clearGroup(this.helpers);
    this.clearGroup(this.handleGroup);
    this.snapLabel.remove();
    this.section.dispose();
    this.materials.dispose();
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }

  private onStore(s: CadState, p: CadState): void {
    // A handle drag is work in the view: from then on the camera is not refitted on its own
    // (the user gets the "Neu einpassen" button instead).
    if (s.drag && !p.drag) this.autoFit = false;
    if (s.theme !== p.theme) {
      // CSS variables change with the theme attribute; read them after the style update.
      requestAnimationFrame(() => this.applyTheme());
    }
    if (s.section !== p.section) {
      if (s.section.on !== p.section.on) this.requestBuild();
      else if (s.section.on) {
        this.section.update(s.section);
        this.updateSnapLabelForSection(s.section);
        this.requestRender();
      }
    }
    if (
      s.shown !== p.shown ||
      s.cursor !== p.cursor ||
      s.editorHover !== p.editorHover ||
      s.allGrids !== p.allGrids ||
      s.meshRevision !== p.meshRevision ||
      s.viewHover !== p.viewHover ||
      s.docRevision !== p.docRevision ||
      s.drag !== p.drag ||
      s.hiddenParts !== p.hiddenParts
    )
      this.requestBuild();
    else if (s.hoverHandle !== p.hoverHandle) {
      this.buildHandles();
      this.requestRender();
    }
  }

  applyTheme(): void {
    const pal = readPalette();
    this.materials.dispose();
    this.materials = new Materials(pal);
    this.renderer.setClearColor(new THREE.Color(pal.background), 1);
    this.section?.setColors(pal.cap, pal.edge, pal.accent);
    this.handleColors = readHandleColors();
    this.requestBuild();
  }

  private resize(): void {
    const w = Math.max(1, this.container.clientWidth);
    const h = Math.max(1, this.container.clientHeight);
    this.renderer.setSize(w, h, false);
    this.renderer.domElement.style.width = `${w}px`;
    this.renderer.domElement.style.height = `${h}px`;
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.materials.setResolution(w, h);
    // The parameter bar appears with the module scene: keep the automatic fit.
    if (this.autoFit) this.fitAll();
    this.requestRender();
  }

  requestRender(): void {
    if (this.renderQueued || this.disposed) return;
    this.renderQueued = true;
    requestAnimationFrame(() => {
      this.renderQueued = false;
      this.render();
    });
  }

  private requestBuild(): void {
    if (this.buildQueued || this.disposed) return;
    this.buildQueued = true;
    requestAnimationFrame(() => {
      this.buildQueued = false;
      this.build();
      this.render();
    });
  }

  render(): void {
    if (this.disposed) return;
    const r = this.renderer;
    const w = this.container.clientWidth;
    const h = this.container.clientHeight;
    r.setScissorTest(false);
    r.setViewport(0, 0, w, h);
    r.render(this.scene3, this.camera);
    // Axis gizmo in the lower left corner.
    const s = Math.min(120, Math.floor(Math.min(w, h) / 4));
    this.gizmo.group.quaternion.copy(this.camera.quaternion).invert();
    r.clearDepth();
    r.setScissorTest(true);
    r.setScissor(6, 6, s, s);
    r.setViewport(6, 6, s, s);
    r.render(this.gizmo.scene, this.gizmo.camera);
    r.setScissorTest(false);
    r.setViewport(0, 0, w, h);
    this.frames++;
    this.noteDrawn();
    this.noteDrag();
  }

  /** Drag latencies: Drag message → scene drawn (coarse), release → fine root mesh drawn. */
  private noteDrag(): void {
    const s = this.lastShown;
    if (!s || !s.complete) return;
    const rootKey = s.root !== null ? s.nodes[s.root].mesh_key : null;
    const coarse = this.meshes.isCoarse(rootKey);
    let metrics: CadState["metrics"] | null = null;
    const st = store.getState();
    for (const [v, t] of bridge.dragTimes) {
      if (v > s.version) continue;
      bridge.dragTimes.delete(v);
      if (v !== s.version) continue;
      metrics ??= { ...st.metrics };
      metrics.dragToScene = [...metrics.dragToScene, { ms: performance.now() - t, coarse }].slice(-500);
    }
    const end = bridge.dragEnd;
    if (end && st.drag === null && s.version >= end.version && !coarse && !this.missingMeshes) {
      bridge.dragEnd = null;
      metrics ??= { ...st.metrics };
      metrics.dragEndToFine = [...metrics.dragEndToFine, performance.now() - end.at].slice(-100);
    }
    if (metrics) store.setState({ metrics });
  }

  /** Records the time from an edit until its scene was drawn (metrics). */
  private noteDrawn(): void {
    const s = this.lastShown;
    if (!s) return;
    const key = `${s.version}|${s.complete}|${s.context.module ?? ""}`;
    if (this.drawnVersions.has(key)) return;
    this.drawnVersions.add(key);
    if (!s.complete) return;
    const st = store.getState();
    const now = performance.now();
    const metrics = { ...st.metrics };
    let changed = false;
    if (metrics.firstSceneMs === null && s.root !== null) {
      metrics.firstSceneMs = now;
      changed = true;
    }
    const t = bridge.editTimes.get(s.version);
    if (t !== undefined) {
      metrics.editToScene = [...metrics.editToScene, now - t].slice(-200);
      bridge.editTimes.delete(s.version);
      changed = true;
    }
    if (changed) store.setState({ metrics });
  }

  // ---------------------------------------------------------------- build

  private clearGroup(g: THREE.Group): void {
    for (const c of [...g.children]) {
      g.remove(c);
      c.traverse((o) => {
        const any = o as THREE.Object3D & { geometry?: THREE.BufferGeometry; material?: THREE.Material; userData: { ownGeometry?: boolean; ownMaterial?: boolean } };
        if (any.userData.ownGeometry && any.geometry) any.geometry.dispose();
        if (any.userData.ownMaterial && any.material) any.material.dispose();
      });
    }
  }

  private build(): void {
    const st = store.getState();
    const scene = st.shown;
    this.clearGroup(this.content);
    this.clearGroup(this.overlay);
    this.materials.releaseFat();
    this.pickables = [];
    this.capBodies = [];
    this.lastShown = scene;
    if (!scene) {
      this.section.clear();
      this.lastPlan = null;
      this.lastGrid = null;
      this.lastHandles = [];
      this.buildHandles();
      this.updateHelpers(null);
      return;
    }
    const doc = bridge.doc();
    const idx = bridge.index(scene);
    const sel = idx.select(doc, st.cursor);
    const hover = st.editorHover !== null ? idx.nodesAt(doc, st.editorHover).nodes : [];
    const hoverRefIdx = st.editorHover !== null ? idx.refAt(st.editorHover) : null;
    const hoverRef = hoverRefIdx !== null ? scene.refs[hoverRefIdx] : null;
    const hoverAnchor = hoverRef && hoverRef.target.kind === "anchor" ? { inst: hoverRef.target.inst, anchor: hoverRef.target.anchor } : null;
    const vh = st.viewHover && st.viewHover.sceneVersion === scene.version ? st.viewHover.nodes : [];
    const hidden = (scene.parts ?? []).filter((pt) => st.hiddenParts.includes(pt.name)).map((pt) => pt.node);
    const p = makePlan({ scene, cursor: sel.nodes, hover, hoverAnchor, picked: vh, hidden });
    this.lastPlan = p;
    this.lastSelection = sel;
    this.lastHoverNodes = hover;
    this.client.ensureMeshes(scene, planNodes(p));

    const hull = modelHull(scene);
    this.modelBox = viewBox(scene, hull);
    const g = gridOverlay({ scene, hull, all: st.allGrids, refs: sel.refs, grids: sel.grids, hoverRef: hoverRefIdx });
    this.lastGrid = g;

    this.lastEdgeKinds = {};
    this.lastZoneEdges = null;
    this.lastOpenFaces = [];
    let missing = 0;
    // Base: the model of the current module. While a node's mesh is missing, the base stays full.
    const itemsReady = p.items.filter((i) => this.meshes.has(scene.nodes[i.node].mesh_key));
    const cursorMissing = p.items.some((i) => i.mode === "full" && !this.meshes.has(scene.nodes[i.node].mesh_key));
    if (p.base) {
      const mode = p.base.mode === "dim" && cursorMissing ? "full" : p.base.mode;
      this.baseMode = mode;
      if (!this.addMesh(scene, { ...p.base, mode })) missing++;
    } else this.baseMode = null;
    for (const it of itemsReady) this.addMesh(scene, it);
    missing += p.items.length - itemsReady.length;
    this.missingMeshes = missing;

    const diag = Math.max(1, boxDiagonal(this.modelBox));
    const pal = this.materials.palette;
    const countEdge = (k: string) => (this.lastEdgeKinds[k] = (this.lastEdgeKinds[k] ?? 0) + 1);
    for (const e of p.edges) {
      this.addFatPolyline(e.points, e.kind === "error" ? pal.errorEdge : pal.selectedEdge, 3.5, false, false, `edge:${e.kind}`);
      countEdge(e.kind);
    }
    if (p.zoneEdges) {
      // Cursor on a zone: only the block's edges in this zone.
      const q = p.zoneEdges;
      const vol = this.volume(scene, q.zone);
      if (vol) {
        const ck = `e|${scene.nodes[q.zone].mesh_key}|${this.meshes.isCoarse(scene.nodes[q.zone].mesh_key)}|${scene.nodes[q.zone].display_world.join(",")}|${q.select}|${JSON.stringify(q.edges)}`;
        const mine = this.cached(ck, () => edgesInZone(q.edges, vol.v, q.select, vol.eps));
        for (const e of mine) {
          this.addFatPolyline(e, pal.selectedEdge, 3.5, false, false, "edge:selected");
          countEdge("selected");
        }
        this.lastZoneEdges = { zone: q.zone, inZone: mine.length, total: q.edges.length };
      } else missing++;
    }
    for (const q of p.openFaces) {
      // shell: faces of the body in the zones are opened (coloured).
      const body = this.meshes.get(scene.nodes[q.body].mesh_key)?.data;
      const vols = q.zones.map((z) => this.volume(scene, z));
      if (!body || vols.some((v) => !v)) {
        missing++;
        continue;
      }
      const eps = Math.max(...vols.map((v) => v!.eps));
      const ck = `f|${scene.nodes[q.body].mesh_key}|${this.meshes.isCoarse(scene.nodes[q.body].mesh_key)}|${q.matrix.join(",")}|${q.select}|${q.zones.map((z) => `${scene.nodes[z].mesh_key}@${scene.nodes[z].display_world.join(",")}`).join(";")}`;
      const faces = this.cached(ck, () => facesInZones(body, q.matrix, vols.map((v) => v!.v), q.select, eps));
      this.addFaceSubset(body.positions, body.indices, [...faces.values()].flat(), q.matrix, pal.openFace);
      this.lastOpenFaces.push({ shell: q.shell, faces: faces.size, engine: scene.nodes[q.shell].opened_faces });
    }
    for (const gd of p.guides) {
      this.addFatPolyline(gd.points, pal.guide, gd.kind === "net" ? 1.6 : 2.6, false, gd.dashed, `guide:${gd.kind}`);
    }
    this.missingMeshes = missing;
    for (const f of p.frames) this.addFrame(f.matrix, f.dashed, diag * 0.18);
    if (p.frames.length === 2) {
      const a = new THREE.Vector3().setFromMatrixPosition(m4(p.frames[0].matrix));
      const b = new THREE.Vector3().setFromMatrixPosition(m4(p.frames[1].matrix));
      const d = b.clone().sub(a);
      if (d.length() > 1e-6) {
        const arrow = new THREE.ArrowHelper(d.clone().normalize(), a, d.length(), new THREE.Color(this.materials.palette.accent).getHex(), Math.min(d.length() * 0.25, diag * 0.04), Math.min(d.length() * 0.12, diag * 0.02));
        arrow.renderOrder = 30;
        arrow.traverse((o) => {
          const mm = (o as THREE.Mesh).material as THREE.Material | undefined;
          if (mm) mm.depthTest = false;
        });
        arrow.userData.ownGeometry = true;
        this.overlay.add(arrow);
      }
    }
    this.addGrid(g, diag);
    this.addAnchors(p, diag);
    this.lastHandles = this.computeHandles(scene, sel, diag);
    this.buildHandles();
    this.updateHelpers(scene);
    this.applySection(st.section);
    this.maybeFit(scene);
    // The face under a resting mouse may belong to another line in the new scene.
    if (this.pointer && scene !== this.pickedScene) {
      this.pickedScene = scene;
      const pt = this.pointer;
      queueMicrotask(() => this.setViewHover(this.pick(pt.x, pt.y)));
    }
  }

  private addMesh(scene: Scene, it: { node: number; mode: Plan["items"][number]["mode"]; matrix: Mat4; flat: boolean }): boolean {
    const n = scene.nodes[it.node];
    const key = n.mesh_key;
    if (!key || !this.meshes.has(key)) return false;
    const style = this.materials.style(it.mode, it.flat, it.mode === "full" ? n.color : null);
    const group = new THREE.Group();
    group.matrixAutoUpdate = false;
    group.matrix.copy(m4(it.matrix));
    group.matrixWorldNeedsUpdate = true;
    if (style.surface) {
      const geo = this.meshes.geometry(key);
      if (geo && geo.index && geo.index.count > 0) {
        const mesh = new THREE.Mesh(geo, style.surface);
        mesh.renderOrder = style.renderOrder;
        mesh.userData = { meshKey: key, node: it.node };
        group.add(mesh);
        if (style.pickable) this.pickables.push(mesh);
        // Opaque solids get a section cap (transparent ones are only clipped).
        if (it.mode === "full" && !it.flat && !style.surface.transparent) this.capBodies.push({ group, geometry: geo, key });
      }
    }
    if (it.mode === "picked" || it.mode === "profile") {
      // Node under the mouse: thick accent edges (2.5 px); profiles: thick, through the result.
      const segs = this.meshes.get(key)?.data.edge_segments;
      if (segs && segs.length >= 6) {
        const geo = new LineSegmentsGeometry();
        geo.setPositions(segs);
        const prof = it.mode === "profile";
        const mat = this.materials.fat(prof ? this.materials.palette.profile : this.materials.palette.accent, prof ? 2.8 : 2.5, !prof);
        mat.resolution.set(this.container.clientWidth, this.container.clientHeight);
        const l = new LineSegments2(geo, mat);
        l.renderOrder = style.renderOrder;
        l.userData.ownGeometry = true;
        group.add(l);
      }
    } else if (style.edges) {
      const eg = this.meshes.edges(key);
      if (eg && (eg.getAttribute("position")?.count ?? 0) > 0) {
        const lines = new THREE.LineSegments(eg, style.edges);
        lines.renderOrder = style.renderOrder + 1;
        group.add(lines);
      }
    }
    group.userData = { node: it.node, mode: it.mode };
    group.name = `mode:${it.mode}`;
    this.content.add(group);
    return true;
  }

  private addFatPolyline(points: Vec3[], color: string, width: number, depthTest = false, dashed = false, name = ""): void {
    if (points.length < 2) return;
    const pos: number[] = [];
    for (let i = 0; i + 1 < points.length; i++) pos.push(...points[i], ...points[i + 1]);
    this.addFatSegments(pos, color, width, depthTest, dashed, name);
  }

  private cached<T>(key: string, f: () => T): T {
    if (this.zoneCache.has(key)) return this.zoneCache.get(key) as T;
    if (this.zoneCache.size > 64) this.zoneCache.clear();
    const v = f();
    this.zoneCache.set(key, v);
    return v;
  }

  /** Zone mesh of a node as a volume in context coordinates (cached by key and placement). */
  private volume(scene: Scene, node: number): { v: MeshVolume; eps: number } | null {
    const n = scene.nodes[node];
    const m = this.meshes.get(n.mesh_key)?.data;
    if (!m) return null;
    const key = `${n.mesh_key}|${n.display_world.join(",")}`;
    let v = this.volumes.get(key);
    if (!v) {
      if (this.volumes.size > 64) this.volumes.clear();
      v = new MeshVolume(m.positions, m.indices, n.display_world);
      this.volumes.set(key, v);
    }
    // Tolerance: chordal deviation of the tessellation plus a small share of the model size.
    const eps = Math.max(m.tol * 1.5, boxDiagonal(this.modelBox) * 2e-4, 1e-4);
    return { v, eps };
  }

  /** Coloured triangles of a mesh (opened faces of a shell). */
  private addFaceSubset(positions: Float32Array, indices: Uint32Array, tris: number[], matrix: Mat4, color: string): void {
    if (!tris.length) return;
    const idx = new Uint32Array(tris.length * 3);
    tris.forEach((t, i) => idx.set(indices.subarray(t * 3, t * 3 + 3), i * 3));
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    geo.setIndex(new THREE.BufferAttribute(idx, 1));
    const mat = new THREE.MeshBasicMaterial({ color, transparent: true, opacity: 0.42, side: THREE.DoubleSide, depthWrite: false, polygonOffset: true, polygonOffsetFactor: -2, polygonOffsetUnits: -6 });
    const mesh = new THREE.Mesh(geo, mat);
    mesh.matrixAutoUpdate = false;
    mesh.matrix.copy(m4(matrix));
    mesh.renderOrder = 42;
    mesh.userData.ownGeometry = true;
    mesh.userData.ownMaterial = true;
    mesh.name = "open-faces";
    this.overlay.add(mesh);
  }

  private addFatSegments(pos: number[], color: string, width: number, depthTest = false, dashed = false, name = ""): void {
    if (pos.length < 6) return;
    const geo = new LineSegmentsGeometry();
    geo.setPositions(pos);
    const mat = this.materials.fat(color, width, depthTest, dashed);
    mat.resolution.set(this.container.clientWidth, this.container.clientHeight);
    const l = new LineSegments2(geo, mat);
    if (dashed) l.computeLineDistances();
    l.renderOrder = 40;
    l.userData.ownGeometry = true;
    l.name = name;
    this.overlay.add(l);
  }

  private addFrame(m: Mat4, dashed: boolean, len: number): void {
    const mat = m4(m);
    const o = new THREE.Vector3().setFromMatrixPosition(mat);
    const cols = ["#e5484d", "#30a46c", "#2f80ed"];
    for (let k = 0; k < 3; k++) {
      const axis = new THREE.Vector3(k === 0 ? 1 : 0, k === 1 ? 1 : 0, k === 2 ? 1 : 0).transformDirection(mat).multiplyScalar(len);
      const e = o.clone().add(axis);
      this.addFatSegments([o.x, o.y, o.z, e.x, e.y, e.z], cols[k], dashed ? 2 : 3, false, dashed);
    }
  }

  private addGrid(g: GridOverlay, diag: number): void {
    const pal = this.materials.palette;
    const solid: number[] = [];
    const dashed: number[] = [];
    for (const l of g.lines) {
      const arr = l.dashed ? dashed : solid;
      for (let i = 0; i + 1 < l.points.length; i++) arr.push(...l.points[i], ...l.points[i + 1]);
    }
    const mk = (pos: number[], dash: boolean) => {
      if (!pos.length) return;
      const geo = new THREE.BufferGeometry();
      geo.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
      const mat = dash
        ? new THREE.LineDashedMaterial({ color: pal.grid, dashSize: diag * 0.012, gapSize: diag * 0.008, transparent: true, opacity: 0.9 })
        : new THREE.LineBasicMaterial({ color: pal.grid, transparent: true, opacity: 0.9 });
      const ls = new THREE.LineSegments(geo, mat);
      if (dash) ls.computeLineDistances();
      ls.renderOrder = 25;
      ls.userData.ownGeometry = true;
      ls.name = dash ? "grid-dashed" : "grid-solid";
      this.overlay.add(ls);
    };
    mk(solid, false);
    mk(dashed, true);
    if (g.points.length) {
      const geo = new THREE.BufferGeometry();
      geo.setAttribute("position", new THREE.Float32BufferAttribute(g.points.flat(), 3));
      const pts = new THREE.Points(geo, new THREE.PointsMaterial({ color: pal.grid, size: 5, sizeAttenuation: false, map: dotTexture(), transparent: true, alphaTest: 0.3, depthTest: false }));
      pts.renderOrder = 26;
      pts.userData.ownGeometry = true;
      pts.name = "grid-points";
      this.overlay.add(pts);
    }
    for (const h of g.highlight) {
      if (h.kind === "point") {
        const geo = new THREE.BufferGeometry();
        geo.setAttribute("position", new THREE.Float32BufferAttribute(h.points.flat(), 3));
        const pts = new THREE.Points(geo, new THREE.PointsMaterial({ color: pal.accent, size: 13, sizeAttenuation: false, map: dotTexture(), transparent: true, alphaTest: 0.3, depthTest: false }));
        pts.renderOrder = 50;
        pts.userData.ownGeometry = true;
        pts.name = "grid-highlight-point";
        this.overlay.add(pts);
      } else if (h.kind === "line") {
        this.addFatPolyline(h.points, pal.accent, 4);
      } else {
        this.addFatPolyline(h.points, pal.accent, 3);
        const [a, b, c, d] = h.points;
        const geo = new THREE.BufferGeometry();
        geo.setAttribute("position", new THREE.Float32BufferAttribute([...a, ...b, ...c, ...a, ...c, ...d], 3));
        const mesh = new THREE.Mesh(geo, new THREE.MeshBasicMaterial({ color: pal.accent, transparent: true, opacity: 0.18, side: THREE.DoubleSide, depthWrite: false }));
        mesh.renderOrder = 45;
        mesh.userData.ownGeometry = true;
        mesh.name = "grid-highlight-plane";
        this.overlay.add(mesh);
      }
    }
  }

  private addAnchors(p: Plan, diag: number): void {
    const pal = this.materials.palette;
    const r = Math.max(0.3, diag * 0.012);
    for (const a of p.anchors) {
      const size = a.active ? r * 1.5 : r;
      const geo = new THREE.OctahedronGeometry(size);
      const mat = new THREE.MeshBasicMaterial({ color: a.active ? pal.accent : pal.anchor, depthTest: false, transparent: true, opacity: 0.95 });
      const m = new THREE.Mesh(geo, mat);
      m.position.set(...a.anchor.point);
      m.renderOrder = 60;
      m.userData.ownGeometry = true;
      m.name = `anchor:${a.anchor.text ?? a.anchor.name}`;
      this.overlay.add(m);
      if (a.anchor.dir) {
        const dir = new THREE.Vector3(...a.anchor.dir).normalize();
        const arrow = new THREE.ArrowHelper(dir, new THREE.Vector3(...a.anchor.point), r * 5, new THREE.Color(a.active ? pal.accent : pal.anchor).getHex(), r * 1.6, r * 0.9);
        arrow.renderOrder = 60;
        arrow.traverse((o) => {
          const mm = (o as THREE.Mesh).material as THREE.Material | undefined;
          if (mm) mm.depthTest = false;
        });
        arrow.userData.ownGeometry = true;
        this.overlay.add(arrow);
      }
    }
  }

  // ---------------------------------------------------------------- helpers, camera

  private floorKey = "";

  private updateHelpers(scene: Scene | null): void {
    const box = this.modelBox;
    const ext = isEmpty(box) ? 50 : Math.max(Math.abs(box.min[0]), Math.abs(box.max[0]), Math.abs(box.min[1]), Math.abs(box.max[1]), 10);
    const step = niceStep(ext / 5);
    const size = Math.ceil((ext * 2.4) / step) * step;
    const key = `${size}|${step}|${this.materials.palette.floor}|${scene ? 1 : 0}`;
    if (key === this.floorKey) return;
    this.floorKey = key;
    this.clearGroup(this.helpers);
    const pal = this.materials.palette;
    const grid = new THREE.GridHelper(size, Math.round(size / step), new THREE.Color(pal.floorMajor), new THREE.Color(pal.floor));
    grid.rotation.x = Math.PI / 2;
    (grid.material as THREE.Material).transparent = true;
    (grid.material as THREE.Material).opacity = 0.7;
    (grid.material as THREE.Material).depthWrite = false;
    grid.renderOrder = -1;
    grid.userData.ownGeometry = true;
    grid.name = "floor";
    this.helpers.add(grid);
    const axes = new THREE.AxesHelper(step);
    axes.userData.ownGeometry = true;
    (axes.material as THREE.Material).depthTest = false;
    (axes.material as THREE.Material).transparent = true;
    axes.renderOrder = 1;
    this.helpers.add(axes);
  }

  private makeGizmo() {
    const scene = new THREE.Scene();
    const camera = new THREE.OrthographicCamera(-1.7, 1.7, 1.7, -1.7, 0.1, 10);
    camera.position.set(0, 0, 5);
    const group = new THREE.Group();
    scene.add(group);
    const cols = [0xe5484d, 0x30a46c, 0x2f80ed];
    const labels = ["X", "Y", "Z"];
    for (let k = 0; k < 3; k++) {
      const dir = new THREE.Vector3(k === 0 ? 1 : 0, k === 1 ? 1 : 0, k === 2 ? 1 : 0);
      group.add(new THREE.ArrowHelper(dir, new THREE.Vector3(), 1, cols[k], 0.3, 0.2));
      const sprite = new THREE.Sprite(new THREE.SpriteMaterial({ map: labelTexture(labels[k], cols[k]), depthTest: false }));
      sprite.position.copy(dir.clone().multiplyScalar(1.3));
      sprite.scale.set(0.6, 0.6, 0.6);
      group.add(sprite);
    }
    return { scene, camera, group };
  }

  private maybeFit(scene: Scene): void {
    // Fit when the first complete scene of a newly loaded file or of another module context
    // is shown (scenes older than the file load still belong to the previous file).
    const st = store.getState();
    if (!scene.complete || scene.version < st.loadVersion) return;
    const key = `${st.loadVersion}|${scene.context.module ?? ""}`;
    if (isEmpty(this.modelBox)) return;
    if (key !== this.lastFitKey) {
      this.lastFitKey = key;
      this.fitAll();
      return;
    }
    // The model changed its size clearly (typing, parameter bar): refit while the camera still
    // shows the automatic fit; after the user moved the camera only offer it (button).
    if (st.drag !== null || !this.fitBox) return;
    const refit = needsRefit(this.fitBox, this.modelBox);
    if (refit && this.autoFit) this.fitAll();
    else if (refit !== st.refitHint) store.setState({ refitHint: refit });
  }

  /** "Alles zeigen": frames the shown model, keeping the view direction. */
  fitAll(): void {
    const b = isEmpty(this.modelBox) ? { min: [-25, -25, 0] as Vec3, max: [25, 25, 25] as Vec3 } : this.modelBox;
    const center = new THREE.Vector3((b.min[0] + b.max[0]) / 2, (b.min[1] + b.max[1]) / 2, (b.min[2] + b.max[2]) / 2);
    const radius = Math.max(1, length(sub(b.max, b.min)) / 2);
    const dir = this.camera.position.clone().sub(this.controls.target);
    if (dir.lengthSq() < 1e-9) dir.copy(VIEW_DIRS.iso.dir);
    dir.normalize();
    const fov = (this.camera.fov * Math.PI) / 180;
    const fitH = radius / Math.sin(fov / 2);
    const fitW = radius / Math.sin(Math.atan(Math.tan(fov / 2) * this.camera.aspect));
    const dist = Math.max(fitH, fitW) * 1.08;
    this.autoFit = true;
    this.fitBox = { min: [...b.min] as Vec3, max: [...b.max] as Vec3 };
    if (store.getState().refitHint) store.setState({ refitHint: false });
    this.controls.target.copy(center);
    this.camera.position.copy(center.clone().add(dir.multiplyScalar(dist)));
    this.camera.near = Math.max(0.01, dist / 1000);
    this.camera.far = dist * 100;
    this.camera.updateProjectionMatrix();
    this.controls.update();
    this.requestRender();
  }

  /** Camera from the direction `dir` (towards the target), z up unless looking along z. */
  viewFrom(dir: Vec3): void {
    const d = new THREE.Vector3(...dir).normalize();
    this.camera.position.copy(this.controls.target.clone().add(d.multiplyScalar(100)));
    this.camera.up.set(0, 0, 1);
    this.controls.update();
    this.fitAll();
  }

  setView(v: StandardView): void {
    const d = VIEW_DIRS[v];
    this.camera.position.copy(this.controls.target.clone().add(d.dir.clone().multiplyScalar(100)));
    this.camera.up.copy(d.up);
    this.controls.update();
    this.fitAll();
  }

  // ---------------------------------------------------------------- handles

  private computeHandles(scene: Scene, sel: CursorSelection, diag: number): HandleGeom[] {
    const st = store.getState();
    const idx = bridge.index(scene);
    const hs = handlesForCursor(scene, idx, bridge.doc(), st.cursor, sel);
    const ring = Math.max(1, diag * 0.16);
    const out = hs.map((h) => handleGeom(scene, h, ring, modelHull(scene)));
    // The dragged handle keeps its geometry from the drag start (its literal changes live).
    const dragId = this.drag?.handle ?? null;
    if (dragId !== null) {
      const old = this.lastHandles.find((g) => g.handle.id === dragId);
      const i = out.findIndex((g) => g.handle.id === dragId);
      if (old && i >= 0) out[i] = old;
      else if (old) out.push(old);
    }
    return out;
  }

  private buildHandles(): void {
    this.clearGroup(this.handleGroup);
    buildHandleLayer(this.handleGroup, {
      geoms: this.lastHandles,
      preview: this.preview,
      hovered: store.getState().hoverHandle,
      colors: this.handleColors,
      resolution: { w: this.container.clientWidth, h: this.container.clientHeight },
    });
    const snap = this.preview?.snap ?? null;
    if (snap) {
      const p = this.project(snap.world);
      this.snapLabel.textContent = snap.text;
      this.snapLabel.hidden = !p.visible;
      if (p.visible) {
        const r = this.container.getBoundingClientRect();
        // Left of the point near the right edge.
        const w = this.snapLabel.offsetWidth || snap.text.length * 8 + 14;
        const x = p.x - r.left + 14 + w > r.width - 4 ? p.x - r.left - 14 - w : p.x - r.left + 14;
        this.snapLabel.style.left = `${Math.max(2, x)}px`;
        this.snapLabel.style.top = `${p.y - r.top - 26}px`;
      }
    } else this.snapLabel.hidden = true;
  }

  /** Drag preview (grip position, snap targets); null when the drag ends. */
  setDragPreview(p: DragPreview | null): void {
    this.preview = p;
    this.buildHandles();
    this.requestRender();
  }

  /** Grip under a client position (nearest within the pick radius). */
  handleAt(clientX: number, clientY: number): HandleGeom | null {
    let best: HandleGeom | null = null;
    let bd = GRIP_PX;
    for (const g of this.lastHandles) {
      const p = this.project(g.grip);
      if (!p.visible) continue;
      const d = Math.hypot(p.x - clientX, p.y - clientY);
      if (d <= bd) {
        bd = d;
        best = g;
      }
    }
    return best;
  }

  private onGripDown(e: PointerEvent): void {
    if (e.button === 0 && this.onSectionGripDown(e)) return;
    if (e.button !== 0 || !this.drag) return;
    const g = this.handleAt(e.clientX, e.clientY);
    if (!g) return;
    e.stopPropagation();
    e.preventDefault();
    if (g.handle.locked) {
      store.setState({ notice: `${handleLabel(g.handle)}: berechneter Wert – gesperrt` });
      return;
    }
    this.drag.start(g, this, e);
  }

  /** Mouse ray in context coordinates. */
  ray(clientX: number, clientY: number): Ray {
    this.raycaster.setFromCamera(this.ndc(clientX, clientY), this.camera);
    const o = this.raycaster.ray.origin;
    const d = this.raycaster.ray.direction;
    return { origin: [o.x, o.y, o.z], dir: [d.x, d.y, d.z] };
  }

  /** Direction from the orbit target to the camera (CLI azimuth/elevation). */
  eyeDir(): Vec3 {
    const d = this.camera.position.clone().sub(this.controls.target).normalize();
    return [d.x, d.y, d.z];
  }

  /** Viewing direction (from the camera towards the target). */
  viewDir(): Vec3 {
    const d = new THREE.Vector3();
    this.camera.getWorldDirection(d);
    return [d.x, d.y, d.z];
  }

  // ---------------------------------------------------------------- section

  /** Clipping, caps and contour for the current build. */
  private applySection(sec: SectionState): void {
    const planes = sec.on ? [this.section.plane] : null;
    this.content.traverse((o) => {
      if (o.name === "section-parity") return;
      const m = (o as THREE.Mesh).material as THREE.Material | THREE.Material[] | undefined;
      for (const mm of Array.isArray(m) ? m : m ? [m] : []) {
        if (mm.clippingPlanes !== planes) mm.clippingPlanes = planes;
      }
    });
    if (!sec.on) {
      this.section.clear();
      return;
    }
    const b = isEmpty(this.modelBox) ? { min: [-25, -25, -25] as Vec3, max: [25, 25, 25] as Vec3 } : this.modelBox;
    this.section.build(sec, this.capBodies, b);
  }

  /** Section grip in screen coordinates (null when off or not visible). */
  sectionGripScreen(): { x: number; y: number } | null {
    if (!store.getState().section.on) return null;
    const g = this.section.gripPos;
    const p = this.project([g.x, g.y, g.z]);
    return p.visible ? { x: p.x, y: p.y } : null;
  }

  /** Closest parameter on the line c + u·n to the mouse ray. */
  private lineParam(c: THREE.Vector3, n: THREE.Vector3, clientX: number, clientY: number): number {
    this.raycaster.setFromCamera(this.ndc(clientX, clientY), this.camera);
    const o = this.raycaster.ray.origin;
    const d = this.raycaster.ray.direction;
    const w = c.clone().sub(o);
    const a = n.dot(n);
    const b = n.dot(d);
    const cc = d.dot(d);
    const dd = n.dot(w);
    const ee = d.dot(w);
    const den = a * cc - b * b;
    if (Math.abs(den) < 1e-9) return 0;
    return (b * ee - cc * dd) / den;
  }

  private onSectionGripDown(e: PointerEvent): boolean {
    const st = store.getState();
    const g = this.sectionGripScreen();
    if (!g || Math.hypot(g.x - e.clientX, g.y - e.clientY) > GRIP_PX * 1.6) return false;
    e.stopPropagation();
    e.preventDefault();
    const n = new THREE.Vector3(...st.section.normal);
    this.sectionDrag = { t0: offset(st.section), center: this.section.gripPos.clone(), n, frames: this.frames, start: performance.now(), last: performance.now(), maxGap: 0 };
    this.controls.enabled = false;
    const move = (ev: PointerEvent) => this.onSectionGripMove(ev);
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      this.controls.enabled = true;
      const d = this.sectionDrag;
      this.sectionDrag = null;
      this.snapLabel.hidden = true;
      if (d) {
        const ms = performance.now() - d.start;
        const frames = this.frames - d.frames;
        const m = store.getState().metrics;
        if (ms > 0 && frames > 1) store.setState({ metrics: { ...m, sectionFps: [...(m.sectionFps ?? []), { fps: (frames * 1000) / ms, frames, ms, maxGapMs: d.maxGap }].slice(-50) } });
      }
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
    return true;
  }

  private onSectionGripMove(e: PointerEvent): void {
    const d = this.sectionDrag;
    if (!d) return;
    const now = performance.now();
    d.maxGap = Math.max(d.maxGap, now - d.last);
    d.last = now;
    const st = store.getState();
    const scene = this.lastShown;
    let t = d.t0 + this.lineParam(d.center, d.n, e.clientX, e.clientY);
    let snap: string | null = null;
    if (!e.altKey && scene) {
      const at = (tt: number) => {
        const q = d.center.clone().add(d.n.clone().multiplyScalar(tt - d.t0));
        return this.project([q.x, q.y, q.z]);
      };
      const cur = at(t);
      let best = st.settings.snapPx;
      for (const c of snapCandidates(scene, st.section)) {
        const q = at(c.t);
        const dist = Math.hypot(q.x - cur.x, q.y - cur.y);
        if (dist < best) {
          best = dist;
          t = c.t;
          snap = c.label;
        }
      }
    }
    if (!snap) {
      // Free drag: round like handles (Einstellungen: Schrittweite).
      const step = st.settings.stepMm > 0 ? Math.min(st.settings.stepMm, 0.1) : 0;
      if (step > 0) t = Math.round(t / step) * step;
    }
    store.setState({ section: withOffset(st.section, t, snap) });
  }

  private updateSnapLabelForSection(s: SectionState): void {
    if (!this.sectionDrag || !s.snap) {
      if (this.sectionDrag) this.snapLabel.hidden = true;
      return;
    }
    const g = this.section.gripPos;
    const p = this.project([g.x, g.y, g.z]);
    const r = this.container.getBoundingClientRect();
    this.snapLabel.textContent = s.snap;
    this.snapLabel.hidden = !p.visible;
    this.snapLabel.style.left = `${Math.max(2, p.x - r.left + 14)}px`;
    this.snapLabel.style.top = `${p.y - r.top - 26}px`;
  }

  /** Section state as drawn (test hook). */
  sectionInfo(): { on: boolean; caps: number; contour: number; grip: { x: number; y: number } | null; gripWorld: Vec3; normal: Vec3; point: Vec3; snap: string | null; dragging: boolean } {
    const s = store.getState().section;
    const g = this.section.gripPos;
    return { on: s.on, caps: s.on ? this.capBodies.length : 0, contour: s.on ? this.section.contourSegments : 0, grip: this.sectionGripScreen(), gripWorld: [g.x, g.y, g.z], normal: effectiveNormal(s), point: s.point, snap: s.snap, dragging: this.sectionDrag !== null };
  }

  /** Moves the plane once per frame over `frames` frames and returns the fps (report). */
  async sectionBenchmark(frames = 120): Promise<{ fps: number; frames: number; ms: number }> {
    const s0 = store.getState().section;
    if (!s0.on) return { fps: 0, frames: 0, ms: 0 };
    const b = this.modelBox;
    const n = s0.normal;
    const span = Math.max(1, boxDiagonal(b) * 0.6);
    const start = performance.now();
    for (let i = 0; i < frames; i++) {
      await new Promise<void>((res) => requestAnimationFrame(() => res()));
      store.setState({ section: withOffset(store.getState().section, offset(s0) + Math.sin((i / frames) * Math.PI * 2) * span * 0.3) });
      this.render();
    }
    const ms = performance.now() - start;
    store.setState({ section: s0 });
    return { fps: (frames * 1000) / ms, frames, ms };
  }

  /** Handles drawn for the cursor line with screen positions (test hook). */
  handleViews(): HandleView[] {
    const doc = bridge.doc();
    const s = this.lastShown;
    const idx = s ? bridge.index(s) : null;
    return this.lastHandles.map((g) => {
      const p = this.project(g.grip);
      const from = idx ? idx.range(g.handle.range).from : 0;
      return {
        id: g.handle.id,
        kind: g.handle.kind.kind,
        axis: "axis" in g.handle.kind ? g.handle.kind.axis : null,
        index: g.handle.index,
        style: g.style,
        line: doc.lineAt(Math.min(from, doc.length)).number,
        value: g.handle.value,
        screen: p.visible ? { x: p.x, y: p.y } : null,
        label: handleLabel(g.handle),
      };
    });
  }

  /** Screen position on a handle's ring (angle in degrees) or line (value). */
  handlePoint(id: number, value: number): { x: number; y: number } | null {
    const g = this.lastHandles.find((x) => x.handle.id === id);
    if (!g) return null;
    let w: Vec3;
    if (g.ring) {
      const a = (value * Math.PI) / 180;
      const r = g.ring;
      w = [0, 1, 2].map((k) => r.center[k] + (r.u[k] * Math.cos(a) + r.v[k] * Math.sin(a)) * r.radius) as Vec3;
    } else if (g.line) {
      const l = g.line;
      w = [0, 1, 2].map((k) => l.base[k] + l.dir[k] * value) as Vec3;
    } else return null;
    const p = this.project(w);
    return { x: p.x, y: p.y };
  }

  // ---------------------------------------------------------------- picking

  private ndc(clientX: number, clientY: number): THREE.Vector2 {
    const r = this.renderer.domElement.getBoundingClientRect();
    return new THREE.Vector2(((clientX - r.left) / r.width) * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1);
  }

  /** Face under a client position → nodes of the context module and their editor lines. */
  pick(clientX: number, clientY: number): PickResult | null {
    const scene = this.lastShown;
    if (!scene || !this.pickables.length) return null;
    this.raycaster.setFromCamera(this.ndc(clientX, clientY), this.camera);
    this.content.updateMatrixWorld(true);
    const hits = this.raycaster.intersectObjects(this.pickables, false);
    const sec = store.getState().section;
    for (const h of hits) {
      if (sec.on && !keeps(sec, [h.point.x, h.point.y, h.point.z])) continue;
      const key = h.object.userData.meshKey as string;
      const data = this.meshes.get(key)?.data;
      if (!data || h.faceIndex === undefined || h.faceIndex === null) continue;
      const tag = data.tri_tags[h.faceIndex];
      const idx = bridge.index(scene);
      const point: Vec3 = [h.point.x, h.point.y, h.point.z];
      const nodes = idx.nodesForTag(tag, point);
      if (!nodes.length) continue;
      const lines = idx.linesOf(bridge.doc(), nodes);
      let other: PickResult["other"] = null;
      if (!lines.length) {
        const r = scene.nodes[nodes[0]].range;
        other = { path: rangePath(scene, r, project.main), line: r.line, pos: r.from };
      }
      return { nodes, lines, point, tag, scene, other };
    }
    return null;
  }

  /** Opens a position in another project file (set by the runtime). */
  onOpenLocation: ((path: string, pos: number) => void) | null = null;

  private onPointerMove(e: PointerEvent): void {
    this.pointer = { x: e.clientX, y: e.clientY };
    if (e.buttons !== 0 || this.drag?.active) return;
    const g = this.handleAt(e.clientX, e.clientY);
    const hid = g?.handle.id ?? null;
    if (store.getState().hoverHandle !== hid) store.setState({ hoverHandle: hid });
    const el = this.renderer.domElement;
    el.style.cursor = g ? (g.handle.locked ? "not-allowed" : "grab") : "";
    el.title = g ? gripTitle(g) : "";
    const first = this.hoverQueued === null;
    this.hoverQueued = e;
    if (!first) return;
    requestAnimationFrame(() => {
      const ev = this.hoverQueued;
      this.hoverQueued = null;
      if (!ev) return;
      const r = this.pick(ev.clientX, ev.clientY);
      this.setViewHover(r);
    });
  }

  private setViewHover(r: PickResult | null): void {
    const cur = store.getState().viewHover;
    const other = r?.other ?? null;
    const co = store.getState().viewHoverOther;
    if ((co?.path ?? null) !== (other?.path ?? null) || co?.line !== other?.line) store.setState({ viewHoverOther: other });
    if (!r) {
      if (cur !== null) store.setState({ viewHover: null });
      return;
    }
    if (cur && cur.sceneVersion === r.scene.version && cur.lines.join() === r.lines.join() && cur.nodes.join() === r.nodes.join()) return;
    store.setState({ viewHover: { lines: r.lines, nodes: r.nodes, sceneVersion: r.scene.version } });
  }

  private onPointerUp(e: PointerEvent): void {
    const d = this.down;
    this.down = null;
    if (!d || e.button !== 0) return;
    if (Math.hypot(e.clientX - d.x, e.clientY - d.y) > 4) return;
    const r = this.pick(e.clientX, e.clientY);
    if (!r) return;
    const idx = bridge.index(r.scene);
    const span = idx.span(r.nodes[0]);
    if (isOff(span) && r.other) this.onOpenLocation?.(r.other.path, r.other.pos);
    else bridge.setCursor(span.from);
  }

  private onDoubleClick(e: MouseEvent): void {
    const r = this.pick(e.clientX, e.clientY);
    if (!r) return;
    const idx = bridge.index(r.scene);
    for (const n of r.nodes) {
      const inst = idx.instanceOfNode(n);
      if (inst === null) continue;
      const name = r.scene.instances[inst].module;
      const def = r.scene.instances[inst].def_range;
      const mod = r.scene.modules.find((m) => m.name === name && (!def || ((m.range.file ?? 0) === (def.file ?? 0) && m.range.from === def.from))) ?? r.scene.modules.find((m) => m.name === name);
      if (!mod) continue;
      if (idx.inFile(mod.body_range)) {
        bridge.setCursor(definitionTarget(bridge.text(), idx.range(mod.body_range)));
      } else {
        // Module of another file (included or used): open that file at the definition.
        const path = rangePath(r.scene, mod.body_range, project.main);
        const text = project.text(path);
        this.onOpenLocation?.(path, definitionTarget(text, { from: mod.body_range.from, to: mod.body_range.to }));
      }
      return;
    }
  }

  /** Client coordinates of a context point (tests). */
  project(p: Vec3): { x: number; y: number; visible: boolean } {
    const v = new THREE.Vector3(...p).project(this.camera);
    const r = this.renderer.domElement.getBoundingClientRect();
    return { x: r.left + ((v.x + 1) / 2) * r.width, y: r.top + ((1 - v.y) / 2) * r.height, visible: v.z > -1 && v.z < 1 && Math.abs(v.x) <= 1 && Math.abs(v.y) <= 1 };
  }

  /** Scans the canvas for a point whose face belongs to `line` (tests). */
  findPointForLine(line: number, step = 6): { x: number; y: number } | null {
    const r = this.renderer.domElement.getBoundingClientRect();
    let best: { x: number; y: number; d: number } | null = null;
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    for (let y = r.top + step; y < r.bottom - step; y += step) {
      for (let x = r.left + step; x < r.right - step; x += step) {
        const p = this.pick(x, y);
        if (p && p.lines.length === 1 && p.lines[0] === line) {
          // Prefer points with the same answer around them (away from edges).
          const ok = [[-3, 0], [3, 0], [0, -3], [0, 3]].every(([dx, dy]) => this.pick(x + dx, y + dy)?.lines.join() === String(line));
          if (!ok) continue;
          const d = Math.hypot(x - cx, y - cy);
          if (!best || d < best.d) best = { x, y, d };
        }
      }
    }
    return best ? { x: best.x, y: best.y } : null;
  }

  /** Rotates the camera for `frames` animation frames and returns the achieved fps. */
  async orbitBenchmark(frames = 120): Promise<{ fps: number; frames: number; ms: number }> {
    const target = this.controls.target.clone();
    const off = this.camera.position.clone().sub(target);
    const start = performance.now();
    for (let i = 0; i < frames; i++) {
      await new Promise<void>((res) => requestAnimationFrame(() => res()));
      off.applyAxisAngle(new THREE.Vector3(0, 0, 1), (2 * Math.PI) / frames);
      this.camera.position.copy(target.clone().add(off));
      this.camera.lookAt(target);
      this.render();
    }
    const ms = performance.now() - start;
    this.controls.update();
    return { fps: (frames * 1000) / ms, frames, ms };
  }

  /**
   * Shows or hides drawn objects by name prefix (`open-faces`, `guide:path`, `edge:selected`,
   * `mode:profile`, …) and renders; returns how many were found (tests: pixel difference).
   */
  setLayerVisible(prefix: string, visible: boolean): number {
    let n = 0;
    for (const g of [this.content, this.overlay]) {
      for (const o of g.children) {
        if (o.name.startsWith(prefix)) {
          o.visible = visible;
          n++;
        }
      }
    }
    this.render();
    return n;
  }

  /** Summary of what is drawn (test hook). */
  viewState(): ViewState {
    const s = this.lastShown;
    const p = this.lastPlan;
    const g = this.lastGrid;
    const doc = bridge.doc();
    const idx = s ? bridge.index(s) : null;
    const lineOf = (n: number) => (idx && !isOff(idx.span(n)) ? doc.lineAt(Math.min(idx.span(n).from, doc.length)).number : 0);

    return {
      sceneVersion: s?.version ?? null,
      complete: s?.complete ?? false,
      context: s?.context.module ?? null,
      selection: this.lastSelection,
      hoverNodes: this.lastHoverNodes,
      plan: {
        base: p?.base ? this.baseMode : null,
        items: (p?.items ?? []).map((i) => ({ node: i.node, mode: i.mode, label: s!.nodes[i.node].label, line: lineOf(i.node), flat: i.flat, why: i.why })),
        edges: p?.edges.length ?? 0,
        frames: p?.frames.length ?? 0,
        anchors: p?.anchors.length ?? 0,
        activeAnchors: p?.anchors.filter((a) => a.active).length ?? 0,
        status: p?.status ?? null,
        guides: (p?.guides ?? []).reduce<Record<string, number>>((m, g) => ((m[g.kind] = (m[g.kind] ?? 0) + 1), m), {}),
        guidePoints: (p?.guides ?? []).map((g) => ({ kind: g.kind, points: g.points })),
      },
      edgeKinds: this.lastEdgeKinds,
      zoneEdges: this.lastZoneEdges,
      openFaces: this.lastOpenFaces,
      camera: {
        autoFit: this.autoFit,
        refitHint: store.getState().refitHint,
        distance: this.camera.position.distanceTo(this.controls.target),
        target: [this.controls.target.x, this.controls.target.y, this.controls.target.z],
      },
      grid: {
        lines: g?.lines.length ?? 0,
        dashed: g?.lines.filter((l) => l.dashed).length ?? 0,
        points: g?.points.length ?? 0,
        highlight: g?.highlight.map((h) => h.kind) ?? [],
      },
      missingMeshes: this.missingMeshes,
      objects: this.content.children.length,
    };
  }
}

function gripTitle(g: HandleGeom): string {
  const h = g.handle;
  const what = handleLabel(h);
  if (h.locked) return `${what}: berechneter Wert – gesperrt`;
  const how = h.via_default ? "Standardwert des Parameters" : h.via_var ? "Variable" : h.comps.every((c) => c === null) ? "Referenz (Snap, Alt: Zahlen)" : "Literal";
  return `${what} (${how}) – ziehen ändert den Code; Alt: frei`;
}

function niceStep(x: number): number {
  const p = Math.pow(10, Math.floor(Math.log10(Math.max(x, 1e-6))));
  const f = x / p;
  return (f < 1.5 ? 1 : f < 3.5 ? 2 : f < 7.5 ? 5 : 10) * p;
}

let dot: THREE.Texture | null = null;
function dotTexture(): THREE.Texture {
  if (dot) return dot;
  const c = document.createElement("canvas");
  c.width = c.height = 32;
  const g = c.getContext("2d")!;
  g.fillStyle = "#fff";
  g.beginPath();
  g.arc(16, 16, 14, 0, Math.PI * 2);
  g.fill();
  dot = new THREE.CanvasTexture(c);
  return dot;
}

function labelTexture(text: string, color: number): THREE.Texture {
  const c = document.createElement("canvas");
  c.width = c.height = 64;
  const g = c.getContext("2d")!;
  g.fillStyle = `#${color.toString(16).padStart(6, "0")}`;
  g.font = "bold 44px system-ui, sans-serif";
  g.textAlign = "center";
  g.textBaseline = "middle";
  g.fillText(text, 32, 34);
  return new THREE.CanvasTexture(c);
}
