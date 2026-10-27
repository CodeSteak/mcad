// 3D view (three.js): draws the display plan of the shown scene, grids and anchors; orbit,
// pan, zoom, standard views; picking (face → tag → node → editor line).

import { t } from "../i18n";
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { LineSegments2 } from "three/addons/lines/LineSegments2.js";
import { LineSegmentsGeometry } from "three/addons/lines/LineSegmentsGeometry.js";
import { LineMaterial } from "three/addons/lines/LineMaterial.js";
import { GTAOPass } from "three/addons/postprocessing/GTAOPass.js";
import { FullScreenQuad } from "three/addons/postprocessing/Pass.js";
import type { EngineClient } from "../engine/client";
import type { AssertMesh, Mat4, MeasureHit, Scene, SrcRange, Vec3 } from "../engine/protocol";
import { definitionTarget } from "../logic/context";
import { type Plan, type SketchDraw, plan as makePlan, planNodes } from "../logic/display";
import { type GridOverlay, gridOverlay, modelHull, viewBox } from "../logic/grids";
import { type HandleGeom, type Ray, handleGeom, handleLabel, handlesForCursor } from "../logic/handles";
import { type Box3, apply, boxDiagonal, emptyBox, isEmpty, length, sub } from "../logic/math";
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
import { displayHex, faceColors, hasFaceColors, vertexColors } from "../logic/colors";
import { type CapBody, SectionLayer } from "./sectionLayer";
import { type SectionState, effectiveNormal, keeps, offset, snapCandidates, withOffset } from "../logic/section";
import type { MeshCache } from "./meshCache";
import type { SketchController, SketchView } from "../app/sketch";
import type { MeasureController, MeasureView } from "../app/measure";
import { pixelSize, planeCamera, rasterLines, rasterSpacing, snapMarker } from "../logic/sketchEditor";
import { STUDIO_THEME, type StudioSettings, aoRadiusFor, lightDirection } from "../logic/studio";

export type StandardView = "iso" | "top" | "front" | "right" | "back" | "left" | "bottom";

const VIEW_DIRS: Record<StandardView, { dir: THREE.Vector3; up: THREE.Vector3 }> = {
  iso: { dir: new THREE.Vector3(1, -1.25, 0.9).normalize(), up: new THREE.Vector3(0, 0, 1) },
  top: { dir: new THREE.Vector3(0, -1e-4, 1).normalize(), up: new THREE.Vector3(0, 0, 1) },
  front: { dir: new THREE.Vector3(0, -1, 0), up: new THREE.Vector3(0, 0, 1) },
  right: { dir: new THREE.Vector3(1, 0, 0), up: new THREE.Vector3(0, 0, 1) },
  back: { dir: new THREE.Vector3(0, 1, 0), up: new THREE.Vector3(0, 0, 1) },
  left: { dir: new THREE.Vector3(-1, 0, 0), up: new THREE.Vector3(0, 0, 1) },
  bottom: { dir: new THREE.Vector3(0, -1e-4, -1).normalize(), up: new THREE.Vector3(0, 0, 1) },
};

/** Camera rotation (world → camera, row-major 3 × 3) for the view cube. */
export type CameraListener = (rot: number[]) => void;

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
  /** Colour regions drawn instead of the base ("Farbregionen zeigen"). */
  regionsDrawn: number;
  /** Face tags with a colour (colours per face). */
  faceColors: number;
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
    /** Places of failed Booleans drawn as red points (context coordinates). */
    failures: Array<{ node: number; point: Vec3 }>;
    status: string | null;
    /** Guide lines by kind (axis, path, net, offset, centerline). */
    guides: Record<string, number>;
    /** Polyline points of the guides (context coordinates). */
    guidePoints: Array<{ kind: string; points: Vec3[] }>;
    /** Sketches drawn for the cursor (segments, the cursor line's segments, help lines). */
    sketches: Array<{ node: number; closed: boolean; segments: number; active: Array<{ stmt: number; kind: string; line: number }>; errors: number; helpers: number; open: number }>;
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

export class Viewer implements DragView, SketchView, MeasureView {
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
  /** Sketch editor: raster, preview, snap marker (rebuilt with every answer). */
  private sketchGroup = new THREE.Group();
  private sketchLabel: HTMLDivElement;
  /** Drag controller (set by the runtime). */
  drag: DragController | null = null;
  /** Sketch editor (set by the runtime). */
  sketch: SketchController | null = null;
  /** Measure mode (set by the runtime). */
  measure: MeasureController | null = null;
  /** Measure mode: hover, picks, measurements (rebuilt when they change). */
  private measureGroup = new THREE.Group();
  /** Results of the geometric assertions: intersection bodies, contacts, gaps (Plan 21 C). */
  private assertGroup = new THREE.Group();
  private assertLabels: Array<{ el: HTMLDivElement; at: Vec3 }> = [];
  private lastAssertDraw: Record<string, number> = {};
  /** Labels of the measurements (DOM, placed every frame). */
  private measureLabels = new Map<number, HTMLDivElement>();
  private measureHoverLabel: HTMLDivElement;
  /** What the measure overlay drew (test hook). */
  lastMeasureDraw: Record<string, number> = {};
  /** What the sketch overlay drew (test hook). */
  lastSketchDraw: Record<string, number> = {};
  /** Handles drawn for the cursor line. */
  lastHandles: HandleGeom[] = [];
  private cameraListeners = new Set<CameraListener>();
  private lastRot = "";
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
  /** Face tag → display colour of the shown scene (colours per face). */
  /** Face colours of the base mesh (count for the debug state). */
  private faceColorCount = 0;
  /** Colour regions drawn instead of the base (`showRegions`). */
  private regionsDrawn = 0;
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
  /** Lights of the standard look (hemisphere + head light). */
  private stdLights: THREE.Light[] = [];
  /** Studio look: key light (shadows), fill light, sky light and the ground. */
  private studioGroup = new THREE.Group();
  private keyLight = new THREE.DirectionalLight(0xffffff, 1);
  private fillLight = new THREE.DirectionalLight(0xffffff, 0.3);
  private skyLight = new THREE.HemisphereLight(0xffffff, 0x8a8f98, 1);
  private ground: THREE.Mesh | null = null;
  private groundKey = "";
  private studioBg: THREE.Texture | null = null;
  /** Ambient occlusion (created on first use). */
  private gtao: GTAOPass | null = null;
  private aoQuad: FullScreenQuad | null = null;
  private aoMaterial: THREE.ShaderMaterial | null = null;
  /** Studio settings in effect. */
  studio: StudioSettings = store.getState().studio;
  /** What the last frame drew in the studio look (test hook). */
  lastStudioDraw: { on: boolean; shadows: boolean; ao: boolean; ground: boolean; aoRadius: number | null; light: Vec3 | null } = { on: false, shadows: false, ao: false, ground: false, aoRadius: null, light: null };
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

    this.scene3.add(this.helpers, this.content, this.overlay, this.handleGroup, this.sketchGroup, this.measureGroup, this.assertGroup);
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
    this.sketchLabel = document.createElement("div");
    this.sketchLabel.className = "snap-label sketch-label";
    this.sketchLabel.setAttribute("data-testid", "sketch-snap-label");
    this.sketchLabel.hidden = true;
    container.appendChild(this.sketchLabel);
    this.measureHoverLabel = document.createElement("div");
    this.measureHoverLabel.className = "snap-label measure-hover";
    this.measureHoverLabel.setAttribute("data-testid", "measure-hover-label");
    this.measureHoverLabel.hidden = true;
    container.appendChild(this.measureHoverLabel);
    const hemi = new THREE.HemisphereLight(0xffffff, 0x8a8f98, 1.6);
    hemi.position.set(0, 0, 1);
    this.scene3.add(hemi);
    const head = new THREE.DirectionalLight(0xffffff, 1.4);
    head.position.set(0.3, 0.2, 1);
    this.camera.add(head);
    this.scene3.add(this.camera);
    this.stdLights = [hemi, head];
    // studio look: lights follow the camera's azimuth (placed every frame), the ground plane
    this.skyLight.position.set(0, 0, 1);
    this.keyLight.shadow.mapSize.set(2048, 2048);
    this.studioGroup.add(this.keyLight, this.keyLight.target, this.fillLight, this.fillLight.target, this.skyLight);
    this.studioGroup.name = "studio";
    this.studioGroup.visible = false;
    this.scene3.add(this.studioGroup);
    // variance shadow maps: a smooth blur for soft shadows (PCF with a large radius is grainy)
    this.renderer.shadowMap.type = THREE.VSMShadowMap;

    this.materials = new Materials(readPalette(), this.studio.on);
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
      this.measure?.leave();
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
    this.clearGroup(this.sketchGroup);
    this.clearGroup(this.measureGroup);
    this.clearGroup(this.assertGroup);
    for (const l of this.assertLabels) l.el.remove();
    this.snapLabel.remove();
    this.sketchLabel.remove();
    this.measureHoverLabel.remove();
    for (const l of this.measureLabels.values()) l.remove();
    this.section.dispose();
    this.gtao?.dispose();
    this.aoQuad?.dispose();
    this.aoMaterial?.dispose();
    this.studioBg?.dispose();
    if (this.ground) {
      this.ground.geometry.dispose();
      (this.ground.material as THREE.Material).dispose();
    }
    this.materials.dispose();
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }

  private onStore(s: CadState, p: CadState): void {
    // A handle drag is work in the view: from then on the camera is not refitted on its own
    // (the user gets the "Neu einpassen" button instead).
    if (s.drag && !p.drag) this.autoFit = false;
    if (s.sketch !== p.sketch) {
      // on/off and another sketch: handles and the sketch drawing; else only the overlay
      if ((s.sketch === null) !== (p.sketch === null) || s.sketch?.answer?.node !== p.sketch?.answer?.node || s.sketch?.tool !== p.sketch?.tool) this.requestBuild();
      this.buildSketch();
      this.requestRender();
    }
    if (s.measure !== p.measure || s.measurements !== p.measurements || s.measureSelected !== p.measureSelected) {
      // on/off: handles are hidden while measuring
      if ((s.measure === null) !== (p.measure === null)) this.requestBuild();
      this.buildMeasure();
      this.requestRender();
    }
    if (s.asserts !== p.asserts || s.assertFocus !== p.assertFocus) {
      this.buildAsserts();
      this.requestRender();
    }
    if (s.studio !== p.studio) this.applyStudio(s.studio);
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
      s.hiddenParts !== p.hiddenParts ||
      s.showRegions !== p.showRegions ||
      s.regions !== p.regions ||
      s.showKeepout !== p.showKeepout
    )
      this.requestBuild();
    else if (s.hoverHandle !== p.hoverHandle) {
      this.buildHandles();
      this.requestRender();
    }
  }

  /** Colour of uncoloured material in the current theme. */
  modelColor(): string {
    return this.materials.palette.model;
  }

  applyTheme(): void {
    const pal = readPalette();
    this.materials.dispose();
    this.materials = new Materials(pal, this.studio.on);
    this.renderer.setClearColor(new THREE.Color(pal.background), 1);
    this.applyStudioLook();
    this.section?.setColors(pal.cap, pal.edge, pal.accent);
    this.handleColors = readHandleColors();
    this.requestBuild();
  }

  /** Switches the studio look or changes its settings. */
  applyStudio(s: StudioSettings): void {
    const was = this.studio;
    this.studio = s;
    // materials depend on the look (roughness, edge opacity) and on the shadow map
    if (was.on !== s.on || was.shadows !== s.shadows) this.applyTheme();
    else {
      this.applyStudioLook();
      this.requestRender();
    }
  }

  /** Lights, shadow map, background and ground of the current studio settings and theme. */
  private applyStudioLook(): void {
    const s = this.studio;
    const on = s.on;
    for (const l of this.stdLights) l.visible = !on;
    this.studioGroup.visible = on;
    const shadows = on && s.shadows;
    this.renderer.shadowMap.enabled = shadows;
    this.keyLight.castShadow = shadows;
    // blur radius in texels ≈ penumbra of a light of radius softness · 5° (as the CLI) at a
    // typical occluder distance of 0.6 · R (the map spans 2.2 · R over 2048 texels)
    const tanTheta = Math.tan((Math.max(0, Math.min(1, s.shadowSoftness)) * 5 * Math.PI) / 180);
    this.keyLight.shadow.radius = Math.min(40, 1 + 560 * tanTheta);
    this.keyLight.shadow.blurSamples = 24;
    const theme = store.getState().theme;
    const th = STUDIO_THEME[theme];
    const gc = new THREE.Color(s.groundColor ?? th.ground);
    const amb = Math.max(0, Math.min(1, s.ambient));
    const k = (1 - amb) * Math.max(0, s.lightIntensity);
    this.keyLight.intensity = k * Math.PI;
    this.fillLight.intensity = 0.22 * k * Math.PI;
    this.skyLight.intensity = amb * Math.PI;
    this.skyLight.color.set(0xffffff);
    this.skyLight.groundColor.setRGB(0.35 + 0.4 * gc.r, 0.35 + 0.4 * gc.g, 0.35 + 0.4 * gc.b);
    // background: vertical gradient (procedural, 1 × 256 canvas)
    this.studioBg?.dispose();
    this.studioBg = null;
    if (on) {
      const [top, bottom] = s.background ?? th.background;
      const cv = document.createElement("canvas");
      cv.width = 1;
      cv.height = 256;
      const ctx = cv.getContext("2d");
      if (ctx) {
        const g = ctx.createLinearGradient(0, 0, 0, 256);
        g.addColorStop(0, top);
        g.addColorStop(1, bottom);
        ctx.fillStyle = g;
        ctx.fillRect(0, 0, 1, 256);
        const tex = new THREE.CanvasTexture(cv);
        tex.colorSpace = THREE.SRGBColorSpace;
        this.studioBg = tex;
      }
    }
    this.scene3.background = this.studioBg;
    this.groundKey = "?";
    this.updateGround();
    const floor = this.helpers.getObjectByName("floor");
    if (floor) floor.visible = !(on && s.ground);
  }

  /** Ground plane at the lowest point of the model, fading out around it (studio look). */
  private updateGround(): void {
    const s = this.studio;
    const box = this.modelBox;
    const want = s.on && s.ground && !isEmpty(box);
    const color = s.groundColor ?? STUDIO_THEME[store.getState().theme].ground;
    const key = want ? `${box.min.join(",")}|${box.max.join(",")}|${color}` : "";
    if (key === this.groundKey) return;
    this.groundKey = key;
    if (this.ground) {
      this.studioGroup.remove(this.ground);
      this.ground.geometry.dispose();
      (this.ground.material as THREE.Material).dispose();
      this.ground = null;
    }
    if (!want) return;
    const diag = Math.max(1, boxDiagonal(box));
    const cx = (box.min[0] + box.max[0]) / 2;
    const cy = (box.min[1] + box.max[1]) / 2;
    const mat = new THREE.MeshStandardMaterial({ color, roughness: 1, metalness: 0, transparent: true, depthWrite: false });
    const fade = { value: new THREE.Vector4(cx, cy, diag * 0.55, diag * 1.5) };
    mat.onBeforeCompile = (sh) => {
      sh.uniforms.groundFade = fade;
      sh.vertexShader = sh.vertexShader
        .replace("#include <common>", "#include <common>\nvarying vec3 vGroundPos;")
        .replace("#include <worldpos_vertex>", "#include <worldpos_vertex>\nvGroundPos = (modelMatrix * vec4(transformed, 1.0)).xyz;");
      sh.fragmentShader = sh.fragmentShader
        .replace("#include <common>", "#include <common>\nvarying vec3 vGroundPos;\nuniform vec4 groundFade;")
        .replace("#include <dithering_fragment>", "#include <dithering_fragment>\n  gl_FragColor.a *= 1.0 - smoothstep(groundFade.z, groundFade.w, distance(vGroundPos.xy, groundFade.xy));");
    };
    mat.customProgramCacheKey = () => "studio-ground";
    const size = diag * 4;
    const g = new THREE.Mesh(new THREE.PlaneGeometry(size, size), mat);
    g.position.set(cx, cy, box.min[2] - diag * 1e-4);
    g.receiveShadow = true;
    g.renderOrder = -2;
    g.name = "studio-ground";
    this.ground = g;
    this.studioGroup.add(g);
  }

  /** Places key and fill light for the current camera (azimuth relative to the camera). */
  private placeStudioLights(): Vec3 {
    const s = this.studio;
    const box = isEmpty(this.modelBox) ? { min: [-25, -25, 0] as Vec3, max: [25, 25, 25] as Vec3 } : this.modelBox;
    const c = new THREE.Vector3((box.min[0] + box.max[0]) / 2, (box.min[1] + box.max[1]) / 2, (box.min[2] + box.max[2]) / 2);
    const r = Math.max(1, boxDiagonal(box) / 2);
    const eye = this.camera.position.clone().sub(this.controls.target).normalize();
    const up = new THREE.Vector3(0, 1, 0).applyQuaternion(this.camera.quaternion);
    const e: Vec3 = [eye.x, eye.y, eye.z];
    const u: Vec3 = [up.x, up.y, up.z];
    const l = lightDirection(e, u, s.lightAzimuth, s.lightElevation);
    this.keyLight.position.set(c.x + l[0] * r * 3, c.y + l[1] * r * 3, c.z + l[2] * r * 3);
    this.keyLight.target.position.copy(c);
    const cam = this.keyLight.shadow.camera;
    // the shadow camera covers the model (the ground only gets shadow where the model casts it)
    cam.left = -r * 1.1;
    cam.right = r * 1.1;
    cam.top = r * 1.1;
    cam.bottom = -r * 1.1;
    cam.near = r * 0.5;
    cam.far = r * 5.5;
    cam.updateProjectionMatrix();
    this.keyLight.shadow.bias = -0.0004;
    this.keyLight.shadow.normalBias = r * 0.004;
    const camEl = (Math.asin(Math.max(-1, Math.min(1, eye.z))) * 180) / Math.PI;
    const f = lightDirection(e, u, 0, camEl + 15);
    this.fillLight.position.set(c.x + f[0] * r * 3, c.y + f[1] * r * 3, c.z + f[2] * r * 3);
    this.fillLight.target.position.copy(c);
    this.keyLight.target.updateMatrixWorld();
    this.fillLight.target.updateMatrixWorld();
    return l;
  }

  /** Ambient occlusion pass of the studio look (lazy). */
  private ensureAo(w: number, h: number): GTAOPass {
    if (!this.gtao) {
      this.gtao = new GTAOPass(this.scene3, this.camera, w, h);
      this.gtao.output = GTAOPass.OUTPUT.Off;
      this.aoMaterial = new THREE.ShaderMaterial({
        uniforms: { tAo: { value: null }, strength: { value: 1 } },
        vertexShader: "varying vec2 vUv; void main() { vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }",
        fragmentShader: "uniform sampler2D tAo; uniform float strength; varying vec2 vUv; void main() { float ao = texture2D(tAo, vUv).r; gl_FragColor = vec4(vec3(clamp(1.0 - strength * (1.0 - ao), 0.0, 1.0)), 1.0); }",
        transparent: true,
        depthTest: false,
        depthWrite: false,
        blending: THREE.CustomBlending,
        blendSrc: THREE.DstColorFactor,
        blendDst: THREE.ZeroFactor,
        blendEquation: THREE.AddEquation,
        blendSrcAlpha: THREE.ZeroFactor,
        blendDstAlpha: THREE.OneFactor,
      });
      this.aoQuad = new FullScreenQuad(this.aoMaterial);
    } else if (this.gtao.width !== w || this.gtao.height !== h) this.gtao.setSize(w, h);
    return this.gtao;
  }

  /**
   * Studio frame with ambient occlusion: the model (without overlays) to the screen, AO of the
   * opaque model multiplied onto it, then the overlays on top (depth of the first pass kept).
   */
  private renderStudioAo(): void {
    const r = this.renderer;
    const size = r.getDrawingBufferSize(new THREE.Vector2());
    const ao = this.ensureAo(size.x, size.y);
    const diag = Math.max(1, boxDiagonal(isEmpty(this.modelBox) ? { min: [0, 0, 0], max: [50, 50, 25] } : this.modelBox));
    const radius = aoRadiusFor(this.studio, diag);
    ao.updateGtaoMaterial({ radius, distanceExponent: 1, thickness: radius, distanceFallOff: 1, scale: 1, samples: 16, screenSpaceRadius: false });
    ao.updatePdMaterial({ lumaPhi: 10, depthPhi: 2, normalPhi: 3, radius: 10, rings: 3, samples: 16 });
    const overlays = [this.overlay, this.handleGroup, this.sketchGroup, this.measureGroup, this.assertGroup];
    const vis = overlays.map((g) => g.visible);
    for (const g of overlays) g.visible = false;
    r.render(this.scene3, this.camera);
    // AO: only opaque model surfaces and the ground (no helpers, no transparent display modes)
    const hidden: THREE.Object3D[] = [];
    const hide = (o: THREE.Object3D) => {
      if (o.visible) {
        o.visible = false;
        hidden.push(o);
      }
    };
    hide(this.helpers);
    hide(this.section.group);
    this.content.traverse((o) => {
      const m = (o as THREE.Mesh).material as THREE.Material | undefined;
      if ((o as THREE.Mesh).isMesh && m && m.transparent) hide(o);
    });
    const bg = this.scene3.background;
    this.scene3.background = null;
    // the shadow map of the first pass stays valid for the AO and overlay passes
    const autoShadow = r.shadowMap.autoUpdate;
    r.shadowMap.autoUpdate = false;
    ao.render(r, null as unknown as THREE.WebGLRenderTarget, null as unknown as THREE.WebGLRenderTarget, 0, false);
    this.scene3.background = bg;
    for (const o of hidden) o.visible = true;
    r.setRenderTarget(null);
    const auto = r.autoClear;
    r.autoClear = false;
    this.aoMaterial!.uniforms.tAo.value = ao.gtaoMap;
    this.aoMaterial!.uniforms.strength.value = Math.max(0, Math.min(2, this.studio.aoStrength));
    this.aoQuad!.render(r);
    // overlays on top of the model's depth (the lights stay on)
    const keep: THREE.Object3D[] = [this.content, this.helpers, this.section.group];
    if (this.ground) keep.push(this.ground);
    const kv = keep.map((g) => g.visible);
    for (const g of keep) g.visible = false;
    overlays.forEach((g, i) => (g.visible = vis[i]));
    this.scene3.background = null;
    r.render(this.scene3, this.camera);
    this.scene3.background = bg;
    keep.forEach((g, i) => (g.visible = kv[i]));
    r.autoClear = auto;
    r.shadowMap.autoUpdate = autoShadow;
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
    this.buildMeasure();
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
    const st = this.studio;
    if (st.on) {
      this.updateGround();
      const light = this.placeStudioLights();
      // AO pauses while a section is shown (the cut faces are stencil caps)
      const ao = st.ao && !store.getState().section.on;
      if (ao) this.renderStudioAo();
      else r.render(this.scene3, this.camera);
      const diag = Math.max(1, boxDiagonal(this.modelBox));
      this.lastStudioDraw = { on: true, shadows: st.shadows, ao, ground: this.ground !== null, aoRadius: ao ? aoRadiusFor(st, diag) : null, light };
    } else {
      r.render(this.scene3, this.camera);
      this.lastStudioDraw = { on: false, shadows: false, ao: false, ground: false, aoRadius: null, light: null };
    }
    this.noteCamera();
    this.placeMeasureLabels();
    this.placeAssertLabels();
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
    // segments (sketches) on the cursor line
    const cline = doc.lineAt(Math.min(st.cursor, doc.length));
    const segmentActive = (r: SrcRange) => {
      const sp = idx.range(r);
      return !isOff(sp) && sp.from <= cline.to && sp.to >= cline.from;
    };
    const p = makePlan({ scene, cursor: sel.nodes, hover, hoverAnchor, picked: vh, hidden, segmentActive, showKeepout: st.showKeepout });
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
    // Colours per face: colour of the creating node below the drawn node (palette colours in
    // the theme variant); the colour regions replace them as soon as they are there.
    this.faceColorCount = p.base ? faceColors(scene, p.base.node, st.theme).count : 0;
    // Base: the model of the current module. While a node's mesh is missing, the base stays full.
    const itemsReady = p.items.filter((i) => this.meshes.has(scene.nodes[i.node].mesh_key));
    const cursorMissing = p.items.some((i) => i.mode === "full" && !this.meshes.has(scene.nodes[i.node].mesh_key));
    this.regionsDrawn = 0;
    if (p.base) {
      const mode = p.base.mode === "dim" && cursorMissing ? "full" : p.base.mode;
      this.baseMode = mode;
      // "Farbregionen zeigen" (default for models with colours, Bericht 12): bodies per colour
      // as the 3MF prints them, instead of the base (full display only)
      const regions = mode === "full" && st.showRegions && hasFaceColors(scene) ? this.regionItems(scene, hidden) : null;
      if (regions) {
        for (const r of regions) this.addRegionMesh(r);
        this.regionsDrawn = regions.length;
      } else if (!this.addMesh(scene, { ...p.base, mode })) missing++;
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
      // the offset contour in the accent colour over the input contour (Plan 17.3)
      const color = gd.kind === "offset" ? pal.accent : pal.guide;
      this.addFatPolyline(gd.points, color, gd.kind === "net" ? 1.6 : 2.6, false, gd.dashed, `guide:${gd.kind}`);
    }
    for (const sk of p.sketches) this.addSketch(sk, diag);
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
    this.addFailures(p, diag);
    this.lastHandles = this.computeHandles(scene, sel, diag);
    this.buildHandles();
    this.buildSketch();
    this.buildMeasure();
    this.buildAsserts();
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

  /** Region meshes of the shown scene, if they are there (`null`: draw the base instead). */
  private regionItems(scene: Scene, hidden: number[]): Array<{ node: number; key: string; color: string; alpha: number; matrix: Mat4 }> | null {
    const st = store.getState();
    const r = st.regions;
    if (!r || r.version !== scene.version || !r.meshes) return null;
    const out: Array<{ node: number; key: string; color: string; alpha: number; matrix: Mat4 }> = [];
    for (const set of r.items) {
      if (hidden.includes(set.node)) continue;
      for (const reg of set.regions) {
        if (!reg.mesh || !this.meshes.has(reg.mesh.key)) return null;
        const info = (scene.colors ?? []).find((c) => c.key === reg.color);
        const color = !info || info.source === "default" ? this.materials.palette.model : displayHex(info.rgba, st.theme);
        // alpha only changes the view (the entry is the RGB value): the first color() gives it
        const alpha = info && info.source !== "default" ? info.rgba[3] : 1;
        out.push({ node: set.node, key: reg.mesh.key, color, alpha, matrix: scene.nodes[set.node].display_world });
      }
    }
    return out.length ? out : null;
  }

  /** One colour region: opaque body in its colour with edges, pickable, with section cap. */
  private addRegionMesh(r: { node: number; key: string; color: string; alpha: number; matrix: Mat4 }): void {
    const c = new THREE.Color(r.color);
    const style = this.materials.style("full", false, [c.r, c.g, c.b, r.alpha]);
    const group = new THREE.Group();
    group.matrixAutoUpdate = false;
    group.matrix.copy(m4(r.matrix));
    group.matrixWorldNeedsUpdate = true;
    const geo = this.meshes.geometry(r.key);
    if (geo && geo.index && geo.index.count > 0 && style.surface) {
      const mesh = new THREE.Mesh(geo, style.surface);
      mesh.renderOrder = style.renderOrder;
      mesh.userData = { meshKey: r.key, node: r.node };
      // studio look: opaque bodies cast and receive shadows
      mesh.castShadow = mesh.receiveShadow = !style.surface.transparent;
      group.add(mesh);
      this.pickables.push(mesh);
      if (!style.surface.transparent) this.capBodies.push({ group, geometry: geo, key: r.key });
    }
    const eg = this.meshes.edges(r.key);
    if (style.edges && eg && (eg.getAttribute("position")?.count ?? 0) > 0) {
      const lines = new THREE.LineSegments(eg, style.edges);
      lines.renderOrder = style.renderOrder + 1;
      group.add(lines);
    }
    group.userData = { node: r.node, mode: "region" };
    group.name = "mode:region";
    this.content.add(group);
  }

  private addMesh(scene: Scene, it: { node: number; mode: Plan["items"][number]["mode"]; matrix: Mat4; flat: boolean }): boolean {
    const n = scene.nodes[it.node];
    const key = n.mesh_key;
    if (!key || !this.meshes.has(key)) return false;
    // Colours per face (colour of the node that created the face), unless the node itself is
    // transparent through `color(…, alpha)`.
    const theme = store.getState().theme;
    const fc = it.mode === "full" && !it.flat && !(n.color && n.color[3] < 1) ? faceColors(scene, it.node, theme) : null;
    const perFace = fc !== null && fc.count > 0;
    const style = this.materials.style(it.mode, it.flat, it.mode === "full" ? n.color : null, perFace);
    const group = new THREE.Group();
    group.matrixAutoUpdate = false;
    group.matrix.copy(m4(it.matrix));
    group.matrixWorldNeedsUpdate = true;
    if (style.surface) {
      const fallback = n.color ? displayHex(n.color, theme) : this.materials.palette.model;
      const geo =
        perFace && fc
          ? this.meshes.coloredGeometry(key, `${fc.sig}|${fallback}`, (d) => vertexColors(d, fc.colorOf, fallback))
          : this.meshes.geometry(key);
      if (geo && geo.index && geo.index.count > 0) {
        const mesh = new THREE.Mesh(geo, style.surface);
        mesh.renderOrder = style.renderOrder;
        mesh.userData = { meshKey: key, node: it.node };
        mesh.castShadow = mesh.receiveShadow = !style.surface.transparent && !it.flat;
        group.add(mesh);
        if (style.pickable) this.pickables.push(mesh);
        // Opaque solids get a section cap (transparent ones are only clipped).
        if (it.mode === "full" && !it.flat && !style.surface.transparent) this.capBodies.push({ group, geometry: geo, key });
      }
    }
    if (it.mode === "keepout") {
      // keep-out volume: dashed outline on top of everything
      const segs = this.meshes.get(key)?.data.edge_segments;
      if (segs && segs.length >= 6) {
        const geo = new LineSegmentsGeometry();
        geo.setPositions(segs);
        const wpp = this.worldPerPx();
        const mat = new LineMaterial({ color: new THREE.Color(this.materials.palette.keepoutEdge).getHex(), linewidth: 1.8, depthTest: false, transparent: true, dashed: true, dashSize: 7 * wpp, gapSize: 4 * wpp });
        mat.resolution.set(this.container.clientWidth, this.container.clientHeight);
        const l = new LineSegments2(geo, mat);
        l.computeLineDistances();
        l.renderOrder = 45;
        l.userData.ownGeometry = true;
        l.userData.ownMaterial = true;
        l.name = "keepout-outline";
        group.add(l);
      }
    } else if (it.mode === "picked" || it.mode === "profile") {
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

  /** Sketch under the cursor: segments in its plane (the cursor line's segment thick in the
   *  accent colour, errors red), help lines, start point with direction, end of open contours. */
  private addSketch(sk: SketchDraw, diag: number): void {
    const pal = this.materials.palette;
    const w = (pts: Vec3[]) => pts.map((q) => apply(sk.matrix, q));
    for (const g of sk.segments) {
      if (g.active) continue;
      const color = g.error ? pal.errorEdge : pal.profile;
      this.addFatPolyline(w(g.points), color, g.error ? 3 : 2.4, false, g.implicit, g.error ? "sketch:error" : "sketch:seg");
    }
    for (const g of sk.segments) {
      if (g.active) this.addFatPolyline(w(g.points), g.error ? pal.errorEdge : pal.accent, 5, false, false, "sketch:active");
    }
    for (const h of sk.helpers) this.addFatPolyline(w(h), pal.guide, 1.4, false, true, "sketch:help");
    const r = diag * 0.012;
    for (const st of sk.starts) {
      const ring: Vec3[] = [];
      for (let i = 0; i <= 24; i++) {
        const a = (i / 24) * Math.PI * 2;
        ring.push([st.point[0] + r * Math.cos(a), st.point[1] + r * Math.sin(a), st.point[2]]);
      }
      this.addFatPolyline(w(ring), pal.accent, 2.2, false, false, "sketch:start");
      // direction of travel: a chevron after the start point
      const l = Math.hypot(st.dir[0], st.dir[1]);
      if (l > 1e-12) {
        const d = [st.dir[0] / l, st.dir[1] / l];
        const tip: Vec3 = [st.point[0] + d[0] * 3 * r, st.point[1] + d[1] * 3 * r, 0];
        const back = (s: number): Vec3 => [tip[0] - d[0] * r + -d[1] * r * s, tip[1] - d[1] * r + d[0] * r * s, 0];
        this.addFatPolyline(w([back(1), tip, back(-1)]), pal.accent, 2.2, false, false, "sketch:dir");
      }
      if (st.open) {
        const e = st.end;
        const sq: Vec3[] = [[e[0] - r, e[1] - r, 0], [e[0] + r, e[1] - r, 0], [e[0] + r, e[1] + r, 0], [e[0] - r, e[1] + r, 0], [e[0] - r, e[1] - r, 0]];
        this.addFatPolyline(w(sq), pal.accent, 2.2, false, false, "sketch:end");
      }
    }
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

  /** Places of failed Booleans (operands touching tangentially): red points through the model. */
  private addFailures(p: Plan, diag: number): void {
    const r = Math.max(0.25, diag * 0.009);
    for (const f of p.failures) {
      const m = new THREE.Mesh(
        new THREE.SphereGeometry(r, 16, 12),
        new THREE.MeshBasicMaterial({ color: this.materials.palette.errorEdge, depthTest: false, transparent: true, opacity: 0.95 }),
      );
      m.position.set(...f.point);
      m.renderOrder = 61;
      m.userData.ownGeometry = true;
      m.name = `failure:${f.node}`;
      this.overlay.add(m);
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
    grid.visible = !(this.studio.on && this.studio.ground);
    this.helpers.add(grid);
    const axes = new THREE.AxesHelper(step);
    axes.userData.ownGeometry = true;
    (axes.material as THREE.Material).depthTest = false;
    (axes.material as THREE.Material).transparent = true;
    axes.renderOrder = 1;
    this.helpers.add(axes);
  }

  /** Tells the view cube about camera rotations (only when it changed). */
  private noteCamera(): void {
    if (!this.cameraListeners.size) return;
    const m = this.camera.matrixWorldInverse.elements;
    // column-major 4 × 4 → row-major 3 × 3 rotation
    const rot = [m[0], m[4], m[8], m[1], m[5], m[9], m[2], m[6], m[10]];
    const key = rot.map((x) => x.toFixed(4)).join(",");
    if (key === this.lastRot) return;
    this.lastRot = key;
    for (const f of this.cameraListeners) f(rot);
  }

  /** Follows the camera rotation (view cube); returns the unsubscribe function. */
  onCamera(f: CameraListener): () => void {
    this.cameraListeners.add(f);
    this.lastRot = "";
    this.requestRender();
    return () => this.cameraListeners.delete(f);
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
    // straight down or up: z can't be "up" – tilt a hair so the front stays at the bottom
    if (Math.abs(d.z) > 0.9999) d.set(0, -1e-4, Math.sign(d.z)).normalize();
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
    // sketch editor: all handles of the sketch (its node in this scene: by the statement)
    const a = st.sketch?.answer ?? null;
    const node = !a?.sketch ? null : a.version === scene.version ? a.node : (scene.nodes.find((n) => n.op_info?.kind === "sketch" && n.range.from === a.sketch!.from && (n.range.file ?? 0) === 0)?.idx ?? null);
    // drawing tools draw (also on existing points): grips only with the select tool
    if (st.sketch && st.sketch.tool !== "select") return [];
    // measuring: clicks pick elements, no grips
    if (st.measure) return [];
    const hs =
      st.sketch && node !== null && scene.nodes[node]?.op_info?.kind === "sketch"
        ? [...scene.handles, ...scene.locked_handles].filter((h) => h.node === node)
        : handlesForCursor(scene, idx, bridge.doc(), st.cursor, sel);
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
    const sk = store.getState().sketch;
    if (sk) {
      // keys go to the sketch editor, not to the code editor
      if (document.activeElement instanceof HTMLElement && document.activeElement.closest(".cm-editor")) document.activeElement.blur();
    }
    const g = this.handleAt(e.clientX, e.clientY);
    if (!g && sk && sk.tool !== "select" && this.sketch) {
      // drawing: the click goes to the tool (no orbiting with the left button)
      e.stopPropagation();
      e.preventDefault();
      this.sketch.click(this.ray(e.clientX, e.clientY), e.altKey);
      return;
    }
    if (!g) return;
    e.stopPropagation();
    e.preventDefault();
    if (g.handle.locked) {
      store.setState({ notice: t("handle.locked", { what: handleLabel(g.handle) }) });
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

  // ---------------------------------------------------------------- sketch editor

  fov(): { deg: number; height: number } {
    return { deg: this.camera.fov, height: Math.max(1, this.container.clientHeight) };
  }

  /** Camera perpendicular onto a sketch plane (Plan 17.12). */
  lookAtPlane(plane: Mat4, min: [number, number], max: [number, number], withModel = false): void {
    if (withModel && !isEmpty(this.modelBox)) {
      // a new sketch: frame the model as well (its box projected onto the plane)
      const inv = new THREE.Matrix4().fromArray(plane).invert();
      const b = this.modelBox;
      for (const x of [b.min[0], b.max[0]])
        for (const y of [b.min[1], b.max[1]])
          for (const z of [b.min[2], b.max[2]]) {
            const v = new THREE.Vector3(x, y, z).applyMatrix4(inv);
            min = [Math.min(min[0], v.x), Math.min(min[1], v.y)];
            max = [Math.max(max[0], v.x), Math.max(max[1], v.y)];
          }
    }
    const c = planeCamera(plane, min, max, this.camera.fov, this.camera.aspect);
    this.autoFit = false;
    this.controls.target.set(...c.target);
    this.camera.position.set(...c.eye);
    this.camera.up.set(...c.up);
    const dist = this.camera.position.distanceTo(this.controls.target);
    this.camera.near = Math.max(0.01, dist / 1000);
    this.camera.far = dist * 100;
    this.camera.updateProjectionMatrix();
    this.controls.update();
    this.buildSketch();
    this.requestRender();
  }

  /** mm per pixel at the orbit target. */
  private worldPerPx(): number {
    return this.camera.position.distanceTo(this.controls.target) * pixelSize(this.camera.fov, this.container.clientHeight);
  }

  private sketchLine(points: Vec3[], color: string, width: number, dashed: boolean, name: string): void {
    if (points.length < 2) return;
    const pos: number[] = [];
    for (let i = 0; i + 1 < points.length; i++) pos.push(...points[i], ...points[i + 1]);
    const geo = new LineSegmentsGeometry();
    geo.setPositions(pos);
    const wpp = this.worldPerPx();
    const mat = new LineMaterial({ color: new THREE.Color(color).getHex(), linewidth: width, depthTest: false, transparent: true, dashed, dashSize: 7 * wpp, gapSize: 5 * wpp });
    mat.resolution.set(this.container.clientWidth, this.container.clientHeight);
    const l = new LineSegments2(geo, mat);
    if (dashed) l.computeLineDistances();
    l.renderOrder = 70;
    l.userData.ownGeometry = true;
    l.userData.ownMaterial = true;
    l.name = name;
    this.sketchGroup.add(l);
    this.lastSketchDraw[name] = (this.lastSketchDraw[name] ?? 0) + 1;
  }

  /** Raster of the drawing plane, preview, help lines, picked points and the snap marker. */
  private buildSketch(): void {
    this.clearGroup(this.sketchGroup);
    this.lastSketchDraw = {};
    const st = store.getState();
    const sk = st.sketch;
    const a = sk?.answer;
    if (!sk || !a) {
      this.sketchLabel.hidden = true;
      return;
    }
    const pal = this.materials.palette;
    const w = (p: [number, number] | number[]): Vec3 => apply(a.plane, [p[0], p[1], 0]);
    const wpp = this.worldPerPx();
    const r = a.raster;
    if (r) {
      // step raster (the step of free positions, coarser when zoomed out), grid lines and points
      const spacing = rasterSpacing(st.settings.stepMm, wpp);
      const minor: number[] = [];
      const major: number[] = [];
      for (const l of rasterLines(r.min, r.max, spacing)) (l.major ? major : minor).push(...w(l.a), ...w(l.b));
      const mk = (pos: number[], color: string, opacity: number, name: string) => {
        if (!pos.length) return;
        const geo = new THREE.BufferGeometry();
        geo.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
        const ls = new THREE.LineSegments(geo, new THREE.LineBasicMaterial({ color, transparent: true, opacity, depthTest: false, depthWrite: false }));
        ls.renderOrder = 24;
        ls.userData.ownGeometry = true;
        ls.userData.ownMaterial = true;
        ls.name = name;
        this.sketchGroup.add(ls);
        this.lastSketchDraw[name] = pos.length / 6;
      };
      mk(minor, pal.floorMajor, 0.55, "sketch-raster");
      mk(major, pal.grey, 0.45, "sketch-raster-major");
      mk(r.lines.flatMap((l) => [...l[0], ...l[1]]), pal.grid, 0.75, "sketch-grid");
      if (r.points.length) {
        const geo = new THREE.BufferGeometry();
        geo.setAttribute("position", new THREE.Float32BufferAttribute(r.points.flat(), 3));
        const pts = new THREE.Points(geo, new THREE.PointsMaterial({ color: pal.grid, size: 6, sizeAttenuation: false, transparent: true, depthTest: false }));
        pts.renderOrder = 26;
        pts.userData.ownGeometry = true;
        pts.userData.ownMaterial = true;
        pts.name = "sketch-grid-points";
        this.sketchGroup.add(pts);
        this.lastSketchDraw["sketch-grid-points"] = r.points.length;
      }
    }
    const dragging = st.drag !== null;
    if (!dragging) {
      for (const g of a.guides) this.sketchLine(g, pal.guide, 1.4, true, "sketch-guide");
      for (const p of a.preview) this.sketchLine(p, pal.accent, 2.6, false, "sketch-preview");
    }
    const sq = (p: [number, number], s: number) => [w([p[0] - s, p[1] - s]), w([p[0] + s, p[1] - s]), w([p[0] + s, p[1] + s]), w([p[0] - s, p[1] + s]), w([p[0] - s, p[1] - s])];
    // scale of the plane (marker sizes are in pixels)
    const unit = Math.max(1e-12, Math.hypot(a.plane[0], a.plane[1], a.plane[2]));
    const px = wpp / unit;
    for (const p of sk.points) this.sketchLine(sq(p.p, 3.5 * px), pal.accent, 2, false, "sketch-picked");
    if (a.current && sk.points.length === 0 && sk.tool !== "select") {
      // end of the open contour at the cursor (where a chain would continue)
      const inv = this.planeLocal(a.plane, a.current);
      this.sketchLine(sq(inv, 3 * px), pal.profile, 2, false, "sketch-current");
    }
    if (sk.tool === "select" && a.segment && a.node !== null) {
      const n = this.lastShown?.nodes[a.node];
      const o = n?.op_info;
      if (o && o.kind === "sketch") {
        for (const c of o.contours) for (const g of c.segments) if (g.range.from === a.segment.from) this.sketchLine(g.points.map((q) => apply(o.plane ?? n!.display_world, q)), pal.accent, 5, false, "sketch-hover");
      }
    }
    const snap = !dragging ? a.snap : null;
    if (snap) {
      this.sketchLine(snapMarker(snap.kind, snap.local, 7 * px).map((q) => w(q)), pal.snap, 2.4, false, `sketch-snap:${snap.kind}`);
      const p = this.project(snap.point);
      this.sketchLabel.textContent = snap.label;
      this.sketchLabel.hidden = !p.visible;
      if (p.visible) {
        const rc = this.container.getBoundingClientRect();
        const lw = this.sketchLabel.offsetWidth || snap.label.length * 8 + 14;
        const x = p.x - rc.left + 14 + lw > rc.width - 4 ? p.x - rc.left - 14 - lw : p.x - rc.left + 14;
        this.sketchLabel.style.left = `${Math.max(2, x)}px`;
        this.sketchLabel.style.top = `${p.y - rc.top + 12}px`;
      }
    } else this.sketchLabel.hidden = true;
  }

  /** Plane coordinates of a context point (on the plane). */
  private planeLocal(plane: Mat4, p: Vec3): [number, number] {
    const m = new THREE.Matrix4().fromArray(plane).invert();
    const v = new THREE.Vector3(...p).applyMatrix4(m);
    return [v.x, v.y];
  }

  /** Screen position of a point of the sketch plane (tests). */
  sketchScreen(local: [number, number]): { x: number; y: number; visible: boolean } | null {
    const a = store.getState().sketch?.answer;
    if (!a) return null;
    return this.project(apply(a.plane, [local[0], local[1], 0]));
  }

  // ---------------------------------------------------------------- measure mode (Plan 18)

  /** The face under a client position for the measure mode (section respected). */
  measureHit(clientX: number, clientY: number): MeasureHit | null {
    if (!this.lastShown || !this.pickables.length) return null;
    this.raycaster.setFromCamera(this.ndc(clientX, clientY), this.camera);
    this.content.updateMatrixWorld(true);
    const sec = store.getState().section;
    for (const h of this.raycaster.intersectObjects(this.pickables, false)) {
      if (sec.on && !keeps(sec, [h.point.x, h.point.y, h.point.z])) continue;
      const key = h.object.userData.meshKey as string;
      const node = h.object.userData.node as number | undefined;
      const data = this.meshes.get(key)?.data;
      if (!data || node === undefined || h.faceIndex === undefined || h.faceIndex === null) continue;
      return { node, face: data.tri_faces[h.faceIndex] ?? 0, point: [h.point.x, h.point.y, h.point.z] };
    }
    return null;
  }

  /** A fat line of the measure overlay (always on top). */
  private measureLine(points: Vec3[], color: string, width: number, dashed: boolean, name: string): void {
    this.measureLineIn(this.measureGroup, points, color, width, dashed, name);
  }

  private measureLineIn(group: THREE.Group, points: Vec3[], color: string, width: number, dashed: boolean, name: string): void {
    if (points.length < 2) return;
    const pos: number[] = [];
    for (let i = 0; i + 1 < points.length; i++) pos.push(...points[i], ...points[i + 1]);
    const geo = new LineSegmentsGeometry();
    geo.setPositions(pos);
    const wpp = this.worldPerPx();
    const mat = new LineMaterial({ color: new THREE.Color(color).getHex(), linewidth: width, depthTest: false, transparent: true, dashed, dashSize: 6 * wpp, gapSize: 4 * wpp });
    mat.resolution.set(this.container.clientWidth, this.container.clientHeight);
    const l = new LineSegments2(geo, mat);
    if (dashed) l.computeLineDistances();
    l.renderOrder = 80;
    l.userData.ownGeometry = true;
    l.userData.ownMaterial = true;
    l.name = name;
    group.add(l);
    const k = name.split(":")[0];
    const draw = group === this.measureGroup ? this.lastMeasureDraw : this.lastAssertDraw;
    draw[k] = (draw[k] ?? 0) + 1;
  }

  /** Point markers (screen-sized dots). */
  private measurePoints(points: Vec3[], color: string, size: number, name: string): void {
    this.measurePointsIn(this.measureGroup, points, color, size, name);
  }

  private measurePointsIn(group: THREE.Group, points: Vec3[], color: string, size: number, name: string): void {
    if (!points.length) return;
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(points.flat(), 3));
    const pts = new THREE.Points(geo, new THREE.PointsMaterial({ color, size, sizeAttenuation: false, map: dotTexture(), alphaTest: 0.5, transparent: true, depthTest: false }));
    pts.renderOrder = 82;
    pts.userData.ownGeometry = true;
    pts.userData.ownMaterial = true;
    pts.name = name;
    group.add(pts);
    const k = name.split(":")[0];
    const draw = group === this.measureGroup ? this.lastMeasureDraw : this.lastAssertDraw;
    draw[k] = (draw[k] ?? 0) + points.length;
  }

  /** Hover highlight, picks and the measurements with their labels. */
  private buildMeasure(): void {
    if (!this.materials) return;
    this.clearGroup(this.measureGroup);
    this.lastMeasureDraw = {};
    const st = store.getState();
    const pal = this.materials.palette;
    const m = st.measure;
    if (m) {
      for (const p of m.picks) {
        for (const l of p.lines) this.measureLine(l, pal.accent, 3.2, false, "measure-pick");
        this.measurePoints([p.point], pal.accent, 11, "measure-pick-point");
      }
      const h = m.hover;
      if (h) {
        for (const l of h.lines) this.measureLine(l, pal.snap, 2.6, false, "measure-hover");
        this.measurePoints([h.point], pal.snap, 10, "measure-hover-point");
      }
    }
    // labels: one per measurement
    const seen = new Set<number>();
    for (const e of st.measurements) {
      seen.add(e.id);
      const sel = st.measureSelected === e.id;
      const color = sel ? pal.accent : pal.measure;
      for (const l of e.m.lines) this.measureLine(l, color, sel ? 3 : 2.2, false, `measure-line:${e.id}`);
      for (const g of e.m.guides) this.measureLine(g, color, 1.3, true, `measure-guide:${e.id}`);
      this.measurePoints(e.m.points, color, sel ? 9 : 7, `measure-point:${e.id}`);
      let el = this.measureLabels.get(e.id);
      if (!el) {
        el = document.createElement("div");
        el.className = "measure-label";
        el.setAttribute("data-testid", `measure-label-${e.id}`);
        const id = e.id;
        el.addEventListener("click", (ev) => {
          ev.stopPropagation();
          this.measure?.select(store.getState().measureSelected === id ? null : id);
        });
        this.container.appendChild(el);
        this.measureLabels.set(e.id, el);
      }
      el.textContent = e.m.label;
      el.title = e.m.text;
      el.classList.toggle("selected", sel);
      el.classList.toggle("stale", st.complete !== null && st.complete.version !== e.version);
    }
    for (const [id, el] of this.measureLabels) {
      if (!seen.has(id)) {
        el.remove();
        this.measureLabels.delete(id);
      }
    }
    this.lastMeasureDraw.labels = seen.size;
    this.placeMeasureLabels();
  }

  /** A mesh of an assertion overlay (context coordinates), on top of the model. */
  private assertMesh(m: AssertMesh, color: string, opacity: number, name: string): void {
    if (!m.indices.length) return;
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(m.positions, 3));
    geo.setIndex(m.indices);
    const mat = new THREE.MeshBasicMaterial({ color, transparent: true, opacity, side: THREE.DoubleSide, depthTest: false, depthWrite: false });
    const mesh = new THREE.Mesh(geo, mat);
    mesh.renderOrder = 60;
    mesh.userData.ownGeometry = true;
    mesh.userData.ownMaterial = true;
    mesh.name = name;
    this.assertGroup.add(mesh);
    if (m.edges.length >= 6) {
      const eg = new LineSegmentsGeometry();
      eg.setPositions(m.edges);
      const lm = new LineMaterial({ color: new THREE.Color(color).getHex(), linewidth: 2.2, depthTest: false, transparent: true });
      lm.resolution.set(this.container.clientWidth, this.container.clientHeight);
      const l = new LineSegments2(eg, lm);
      l.renderOrder = 61;
      l.userData.ownGeometry = true;
      l.userData.ownMaterial = true;
      l.name = `${name}-edges`;
      this.assertGroup.add(l);
    }
    const k = name.split(":")[0];
    this.lastAssertDraw[k] = (this.lastAssertDraw[k] ?? 0) + 1;
  }

  /** Assertion results of the shown scene (Plan 21 C): intersection bodies red, contact faces
   *  and lines green, gaps and too small clearances as dimension lines with a label. */
  private buildAsserts(): void {
    if (!this.materials) return;
    this.clearGroup(this.assertGroup);
    for (const l of this.assertLabels) l.el.remove();
    this.assertLabels = [];
    this.lastAssertDraw = {};
    const st = store.getState();
    const a = st.asserts;
    const shown = this.lastShown;
    if (!a || !shown || a.version !== shown.version) return;
    const pal = this.materials.palette;
    const focus = st.assertFocus && st.assertFocus.version === a.version ? st.assertFocus : null;
    const label = (text: string, at: Vec3, bad: boolean, id: string) => {
      const el = document.createElement("div");
      el.className = `assert-label${bad ? " bad" : ""}`;
      el.setAttribute("data-testid", `assert-label-${id}`);
      el.textContent = text;
      this.container.appendChild(el);
      this.assertLabels.push({ el, at });
    };
    const center = (b: { min: Vec3; max: Vec3 }): Vec3 => [(b.min[0] + b.max[0]) / 2, (b.min[1] + b.max[1]) / 2, (b.min[2] + b.max[2]) / 2];
    for (const r of a.items) {
      if (!r.checked) continue;
      r.pairs.forEach((pr, i) => {
        const id = `${r.node}-${i}`;
        const focused = focus !== null && focus.node === r.node && focus.pair === i;
        // intersections are drawn where they are a violation (not the wanted ones of assert_intersect)
        if (pr.overlap && !pr.ok) this.assertMesh(pr.overlap, pal.overlap, focused ? 0.75 : 0.55, `assert-overlap:${id}`);
        if (pr.contact_mesh) this.assertMesh(pr.contact_mesh, pal.contact, 0.45, `assert-contact:${id}`);
        const lc = pr.ok ? pal.contact : pal.overlap;
        for (const l of pr.contact_lines) {
          if (l.length === 1) this.measurePointsIn(this.assertGroup, l, lc, 9, `assert-point:${id}`);
          else this.measureLineIn(this.assertGroup, l, lc, 3, false, `assert-line:${id}`);
        }
        const gap = pr.problem === "gap" || pr.problem === "too_close" || pr.problem === "apart";
        if (gap && pr.points.length === 2 && (pr.distance ?? 0) > 0) {
          this.measureLineIn(this.assertGroup, pr.points, pal.overlap, focused ? 3.4 : 2.6, false, `assert-gap:${id}`);
          this.measurePointsIn(this.assertGroup, pr.points, pal.overlap, 9, `assert-gap-point:${id}`);
        }
        const at = pr.focus ? center(pr.focus) : pr.points[0];
        if (!at) return;
        const d = (pr.distance ?? 0).toFixed(3).replace(/\.?0+$/, "");
        const text =
          pr.problem === "overlap"
            ? pr.volume
              ? t("assert.overlapVolume", { v: pr.volume.toFixed(2).replace(/\.?0+$/, "") })
              : t("assert.overlap")
            : pr.problem === "too_close"
              ? `${d} mm < ${r.clearance}`
              : pr.problem === "gap"
                ? t("assert.gap", { d })
                : pr.problem === "apart"
                  ? t("assert.apart", { d })
                  : pr.problem === "line_contact"
                    ? t("assert.lineContact")
                    : pr.problem === "point_contact"
                      ? t("assert.pointContact")
                      : pr.contact_area !== null
                        ? t("assert.contact", { a: pr.contact_area.toFixed(2).replace(/\.?0+$/, "") })
                        : null;
        if (text && (!pr.ok || pr.contact === "face")) label(text, at, !pr.ok, id);
      });
    }
    this.lastAssertDraw.labels = this.assertLabels.length;
    this.placeAssertLabels();
  }

  private placeAssertLabels(): void {
    if (!this.assertLabels.length) return;
    const rc = this.container.getBoundingClientRect();
    for (const { el, at } of this.assertLabels) {
      const p = this.project(at);
      el.hidden = !p.visible;
      if (!p.visible) continue;
      const w = el.offsetWidth || 80;
      el.style.left = `${Math.round(Math.min(Math.max(2, p.x - rc.left - w / 2), rc.width - w - 2))}px`;
      el.style.top = `${Math.round(Math.max(2, p.y - rc.top - 30))}px`;
    }
  }

  /** The assertion overlay as drawn (test hook). */
  assertView(): { drawn: Record<string, number>; labels: Array<{ text: string; bad: boolean; visible: boolean }>; keepout: number } {
    let keepout = 0;
    this.content.traverse((o) => {
      if (o.name === "mode:keepout") keepout++;
    });
    return { drawn: this.lastAssertDraw, labels: this.assertLabels.map((l) => ({ text: l.el.textContent ?? "", bad: l.el.classList.contains("bad"), visible: !l.el.hidden })), keepout };
  }

  /** Frames a box (context coordinates): the camera looks at it from its current direction. */
  frameBox(min: Vec3, max: Vec3): void {
    const center = new THREE.Vector3((min[0] + max[0]) / 2, (min[1] + max[1]) / 2, (min[2] + max[2]) / 2);
    const diag = Math.max(length(sub(max, min)), boxDiagonal(this.modelBox) * 0.15, 1);
    const dir = this.camera.position.clone().sub(this.controls.target);
    if (dir.lengthSq() < 1e-9) dir.copy(VIEW_DIRS.iso.dir);
    dir.normalize();
    const fov = (this.camera.fov * Math.PI) / 180;
    const dist = (diag / 2 / Math.sin(fov / 2)) * 2.4;
    this.autoFit = false;
    this.controls.target.copy(center);
    this.camera.position.copy(center.clone().add(dir.multiplyScalar(dist)));
    this.camera.near = Math.max(0.01, dist / 1000);
    this.camera.far = Math.max(this.camera.far, dist * 100);
    this.camera.updateProjectionMatrix();
    this.controls.update();
    this.requestRender();
  }

  /** Places the labels at their anchors (every frame: the camera moves). */
  private placeMeasureLabels(): void {
    const st = store.getState();
    const rc = this.container.getBoundingClientRect();
    // labels do not cover each other: a label that would overlap one placed before moves up
    // or down by its height (the selected one is placed first)
    const placed: Array<{ x: number; y: number; w: number; h: number }> = [];
    const order = [...st.measurements].sort((a, b) => Number(b.id === st.measureSelected) - Number(a.id === st.measureSelected));
    for (const e of order) {
      const el = this.measureLabels.get(e.id);
      if (!el) continue;
      const p = this.project(e.m.at);
      el.hidden = !p.visible;
      if (!p.visible) continue;
      const w = el.offsetWidth || e.m.label.length * 7 + 12;
      const h = el.offsetHeight || 18;
      const x = Math.round(Math.min(Math.max(2, p.x - rc.left - w / 2), rc.width - w - 2));
      let y = Math.round(Math.min(Math.max(2, p.y - rc.top - h - 6), rc.height - h - 2));
      const hits = (yy: number) => placed.some((q) => x < q.x + q.w + 2 && q.x < x + w + 2 && yy < q.y + q.h + 2 && q.y < yy + h + 2);
      for (let k = 1; k <= 8 && hits(y); k++) {
        const dy = (h + 3) * Math.ceil(k / 2) * (k % 2 ? -1 : 1);
        const yy = y + dy;
        if (yy >= 2 && yy <= rc.height - h - 2 && !hits(yy)) {
          y = yy;
          break;
        }
      }
      placed.push({ x, y, w, h });
      el.style.left = `${x}px`;
      el.style.top = `${y}px`;
    }
    const h = st.measure?.hover ?? null;
    const lab = this.measureHoverLabel;
    if (!h) {
      lab.hidden = true;
      return;
    }
    const p = this.project(h.point);
    lab.textContent = h.label;
    lab.hidden = !p.visible;
    if (p.visible) {
      const w = lab.offsetWidth || h.label.length * 8 + 14;
      const x = p.x - rc.left + 14 + w > rc.width - 4 ? p.x - rc.left - 14 - w : p.x - rc.left + 14;
      lab.style.left = `${Math.max(2, x)}px`;
      lab.style.top = `${p.y - rc.top + 12}px`;
    }
  }

  /** The measure overlay as drawn (test hook): counts and label positions. */
  measureView(): { drawn: Record<string, number>; labels: Array<{ id: number; text: string; x: number; y: number; visible: boolean }>; hoverLabel: string | null } {
    const labels = [...this.measureLabels.entries()].map(([id, el]) => {
      const r = el.getBoundingClientRect();
      return { id, text: el.textContent ?? "", x: r.left + r.width / 2, y: r.top + r.height / 2, visible: !el.hidden };
    });
    return { drawn: this.lastMeasureDraw, labels, hoverLabel: this.measureHoverLabel.hidden ? null : this.measureHoverLabel.textContent };
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
      // like a drag: the store update moves the plane, the frame is drawn by the next rAF
      store.setState({ section: withOffset(store.getState().section, offset(s0) + Math.sin((i / frames) * Math.PI * 2) * span * 0.3) });
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
    if (this.sketch?.active) this.sketch.hover(this.ray(e.clientX, e.clientY), e.altKey);
    if (this.measure?.active) {
      // measuring: the engine's element under the mouse is the highlight (no node hover, which
      // would draw the node's own mesh and change what the raycast hits)
      this.measure.hover(e.clientX, e.clientY, e.altKey);
      this.renderer.domElement.style.cursor = "crosshair";
      if (store.getState().viewHover !== null) this.setViewHover(null);
      return;
    }
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
    if (this.measure?.active) {
      // measuring: the click picks (the editor cursor stays)
      this.measure.click(e.clientX, e.clientY, e.altKey);
      return;
    }
    const sk = store.getState().sketch;
    if (sk) {
      // select tool: a click on a segment puts the cursor on its line (Entf deletes it)
      const seg = sk.tool === "select" ? sk.answer?.segment : null;
      const scene = this.lastShown;
      if (seg && scene && sk.answer?.version !== undefined) {
        const m = bridge.mapper.has(sk.answer.version) ? bridge.mapper.mapper(sk.answer.version) : (x: number) => x;
        bridge.setCursor(m(seg.from, 1), false);
      }
      return;
    }
    const r = this.pick(e.clientX, e.clientY);
    if (!r) return;
    const idx = bridge.index(r.scene);
    const span = idx.span(r.nodes[0]);
    if (isOff(span) && r.other) this.onOpenLocation?.(r.other.path, r.other.pos);
    else bridge.setCursor(span.from);
  }

  private onDoubleClick(e: MouseEvent): void {
    if (this.measure?.active) return;
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
      regionsDrawn: this.regionsDrawn,
      faceColors: this.faceColorCount,
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
        failures: (p?.failures ?? []).map((f) => ({ node: f.node, point: f.point })),
        status: p?.status ?? null,
        guides: (p?.guides ?? []).reduce<Record<string, number>>((m, g) => ((m[g.kind] = (m[g.kind] ?? 0) + 1), m), {}),
        guidePoints: (p?.guides ?? []).map((g) => ({ kind: g.kind, points: g.points })),
        sketches: (p?.sketches ?? []).map((k) => ({
          node: k.node,
          closed: k.closed,
          segments: k.segments.length,
          active: k.segments.filter((g) => g.active).map((g) => ({ stmt: g.stmt, kind: g.kind, line: s ? lineOfStmt(s, g.stmt) : 0 })),
          errors: k.segments.filter((g) => g.error).length,
          helpers: k.helpers.length,
          open: k.starts.filter((st) => st.open).length,
        })),
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
  if (h.locked) return t("handle.locked", { what });
  const how = h.via_default ? t("handle.viaDefault") : h.via_var ? t("handle.viaVar") : h.comps.every((c) => c === null) ? t("handle.viaRef") : t("handle.viaLiteral");
  return t("handle.grip", { what, how });
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

/** 1-based line of a statement's first node (debug state). */
function lineOfStmt(scene: Scene, stmt: number): number {
  for (const n of scene.nodes) {
    if (n.op_info?.kind !== "sketch") continue;
    for (const c of n.op_info.contours) for (const g of c.segments) if (g.stmt === stmt) return g.range.line;
  }
  return 0;
}
