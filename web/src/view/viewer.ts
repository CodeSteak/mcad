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
import type { DragController, DragPreview, DragView } from "../app/drag";
import { buildHandleLayer, readHandleColors, type HandleColors } from "./handleLayer";
import { EMPTY_SELECTION, type CursorSelection } from "../logic/sceneIndex";
import { bridge } from "../state/bridge";
import { type CadState, store } from "../state/store";
import { Materials, readPalette } from "./materials";
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
}

export interface ViewState {
  sceneVersion: number | null;
  complete: boolean;
  context: string | null;
  selection: CursorSelection;
  hoverNodes: number[];
  plan: { base: string | null; items: Array<{ node: number; mode: string; label: string; line: number; flat: boolean }>; edges: number; frames: number; anchors: number; activeAnchors: number; status: string | null };
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
  private pickables: THREE.Mesh[] = [];
  private raycaster = new THREE.Raycaster();
  private renderQueued = false;
  private buildQueued = false;
  private unsub: () => void;
  private resizeObs: ResizeObserver;
  private lastFitKey = "";
  /** The camera still shows the last automatic fit (refit when the viewport size changes). */
  private autoFit = false;
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
    this.renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.localClippingEnabled = false;
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
    this.materials.dispose();
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }

  private onStore(s: CadState, p: CadState): void {
    if (s.theme !== p.theme) {
      // CSS variables change with the theme attribute; read them after the style update.
      requestAnimationFrame(() => this.applyTheme());
    }
    if (
      s.shown !== p.shown ||
      s.cursor !== p.cursor ||
      s.editorHover !== p.editorHover ||
      s.allGrids !== p.allGrids ||
      s.meshRevision !== p.meshRevision ||
      s.viewHover !== p.viewHover ||
      s.docRevision !== p.docRevision ||
      s.drag !== p.drag
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
    this.lastShown = scene;
    if (!scene) {
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
    const p = makePlan({ scene, cursor: sel.nodes, hover, hoverAnchor, picked: vh });
    this.lastPlan = p;
    this.lastSelection = sel;
    this.lastHoverNodes = hover;
    this.client.ensureMeshes(scene, planNodes(p));

    const hull = modelHull(scene);
    this.modelBox = viewBox(scene, hull);
    const g = gridOverlay({ scene, hull, all: st.allGrids, refs: sel.refs, grids: sel.grids, hoverRef: hoverRefIdx });
    this.lastGrid = g;

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
    for (const e of p.edges) this.addFatPolyline(e.points, e.kind === "error" ? this.materials.palette.errorEdge : this.materials.palette.selectedEdge, 3.5);
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
      }
    }
    if (it.mode === "picked") {
      // Node under the mouse: thick accent edges (2.5 px).
      const segs = this.meshes.get(key)?.data.edge_segments;
      if (segs && segs.length >= 6) {
        const geo = new LineSegmentsGeometry();
        geo.setPositions(segs);
        const mat = this.materials.fat(this.materials.palette.accent, 2.5, true);
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
    this.content.add(group);
    return true;
  }

  private addFatPolyline(points: Vec3[], color: string, width: number, depthTest = false): void {
    if (points.length < 2) return;
    const pos: number[] = [];
    for (let i = 0; i + 1 < points.length; i++) pos.push(...points[i], ...points[i + 1]);
    this.addFatSegments(pos, color, width, depthTest);
  }

  private addFatSegments(pos: number[], color: string, width: number, depthTest = false, dashed = false): void {
    if (pos.length < 6) return;
    const geo = new LineSegmentsGeometry();
    geo.setPositions(pos);
    const mat = this.materials.fat(color, width, depthTest, dashed);
    mat.resolution.set(this.container.clientWidth, this.container.clientHeight);
    const l = new LineSegments2(geo, mat);
    if (dashed) l.computeLineDistances();
    l.renderOrder = 40;
    l.userData.ownGeometry = true;
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
    if (key === this.lastFitKey || isEmpty(this.modelBox)) return;
    this.lastFitKey = key;
    this.fitAll();
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
    this.controls.target.copy(center);
    this.camera.position.copy(center.clone().add(dir.multiplyScalar(dist)));
    this.camera.near = Math.max(0.01, dist / 1000);
    this.camera.far = dist * 100;
    this.camera.updateProjectionMatrix();
    this.controls.update();
    this.requestRender();
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

  /** Viewing direction (from the camera towards the target). */
  viewDir(): Vec3 {
    const d = new THREE.Vector3();
    this.camera.getWorldDirection(d);
    return [d.x, d.y, d.z];
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
    for (const h of hits) {
      const key = h.object.userData.meshKey as string;
      const data = this.meshes.get(key)?.data;
      if (!data || h.faceIndex === undefined || h.faceIndex === null) continue;
      const tag = data.tri_tags[h.faceIndex];
      const idx = bridge.index(scene);
      const point: Vec3 = [h.point.x, h.point.y, h.point.z];
      const nodes = idx.nodesForTag(tag, point);
      if (!nodes.length) continue;
      return { nodes, lines: idx.linesOf(bridge.doc(), nodes), point, tag, scene };
    }
    return null;
  }

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
    bridge.setCursor(idx.span(r.nodes[0]).from);
  }

  private onDoubleClick(e: MouseEvent): void {
    const r = this.pick(e.clientX, e.clientY);
    if (!r) return;
    const idx = bridge.index(r.scene);
    for (const n of r.nodes) {
      const inst = idx.instanceOfNode(n);
      if (inst === null) continue;
      const name = r.scene.instances[inst].module;
      const mod = r.scene.modules.find((m) => m.name === name);
      if (!mod) continue;
      bridge.setCursor(definitionTarget(bridge.text(), idx.range(mod.body_range)));
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

  /** Summary of what is drawn (test hook). */
  viewState(): ViewState {
    const s = this.lastShown;
    const p = this.lastPlan;
    const g = this.lastGrid;
    const doc = bridge.doc();
    const idx = s ? bridge.index(s) : null;
    const lineOf = (n: number) => (idx ? doc.lineAt(Math.min(idx.span(n).from, doc.length)).number : 0);
    return {
      sceneVersion: s?.version ?? null,
      complete: s?.complete ?? false,
      context: s?.context.module ?? null,
      selection: this.lastSelection,
      hoverNodes: this.lastHoverNodes,
      plan: {
        base: p?.base ? this.baseMode : null,
        items: (p?.items ?? []).map((i) => ({ node: i.node, mode: i.mode, label: s!.nodes[i.node].label, line: lineOf(i.node), flat: i.flat })),
        edges: p?.edges.length ?? 0,
        frames: p?.frames.length ?? 0,
        anchors: p?.anchors.length ?? 0,
        activeAnchors: p?.anchors.filter((a) => a.active).length ?? 0,
        status: p?.status ?? null,
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
