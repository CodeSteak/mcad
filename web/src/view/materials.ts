// Materials per display mode (Plan 9: Darstellungszustände). Colours come from CSS variables
// so light and dark themes match the rest of the UI.

import * as THREE from "three";
import { LineMaterial } from "three/addons/lines/LineMaterial.js";
import type { Mode } from "../logic/display";

export interface Palette {
  background: string;
  model: string;
  edge: string;
  dim: string;
  dimEdge: string;
  accent: string;
  red: string;
  grey: string;
  blue: string;
  ghost: string;
  outline: string;
  selectedEdge: string;
  errorEdge: string;
  grid: string;
  gridUsed: string;
  floor: string;
  floorMajor: string;
  anchor: string;
  flat: string;
  profile: string;
  guide: string;
  openFace: string;
  /** Section caps (like the CLI renderer). */
  cap: string;
  /** Snap marker of the sketch editor (like the snap label). */
  snap: string;
  /** Dimension lines and labels of the measure mode. */
  measure: string;
  /** Keep-out volume of `assert_keepout`: fill and outline. */
  keepout: string;
  keepoutEdge: string;
  /** Intersection body of a violated assertion. */
  overlap: string;
  /** Contact face of `assert_touches`. */
  contact: string;
}

export function readPalette(el: HTMLElement = document.documentElement): Palette {
  const cs = getComputedStyle(el);
  const v = (n: string, d: string) => cs.getPropertyValue(n).trim() || d;
  return {
    background: v("--view-bg", "#f4f5f7"),
    model: v("--view-model", "#c9ced6"),
    edge: v("--view-edge", "#3a3f47"),
    dim: v("--view-dim", "#9aa1ab"),
    dimEdge: v("--view-dim-edge", "#9aa1ab"),
    accent: v("--view-accent", "#2f80ed"),
    red: v("--view-red", "#e5484d"),
    grey: v("--view-grey", "#8a8f98"),
    blue: v("--view-blue", "#3b82f6"),
    ghost: v("--view-ghost", "#9aa4b2"),
    outline: v("--view-outline", "#6b7280"),
    selectedEdge: v("--view-selected-edge", "#f59e0b"),
    errorEdge: v("--view-error-edge", "#e5484d"),
    grid: v("--view-grid", "#8b5cf6"),
    gridUsed: v("--view-grid-used", "#7c3aed"),
    floor: v("--view-floor", "#d9dce1"),
    floorMajor: v("--view-floor-major", "#c4c8cf"),
    anchor: v("--view-anchor", "#f59e0b"),
    flat: v("--view-flat", "#2f80ed"),
    profile: v("--view-profile", "#0b7285"),
    guide: v("--view-guide", "#d6336c"),
    openFace: v("--view-open-face", "#f08c00"),
    cap: v("--view-cap", "#db5c3d"),
    snap: v("--view-snap", "#237a35"),
    measure: v("--view-measure", "#8a5a00"),
    keepout: v("--view-keepout", "#ca8a04"),
    keepoutEdge: v("--view-keepout-edge", "#8a5a00"),
    overlap: v("--view-overlap", "#e03131"),
    contact: v("--view-contact", "#2f9e44"),
  };
}

export interface ModeStyle {
  surface: THREE.Material | null;
  edges: THREE.LineBasicMaterial | null;
  renderOrder: number;
  pickable: boolean;
}

/** Shared materials for one palette. */
export class Materials {
  readonly palette: Palette;
  private styles = new Map<string, ModeStyle>();
  private fullByColor = new Map<string, THREE.Material>();
  private fatLines: LineMaterial[] = [];
  /** Studio look: glossier surfaces (highlights of the key light), lighter edges. */
  readonly studio: boolean;

  constructor(palette: Palette, studio = false) {
    this.palette = palette;
    this.studio = studio;
  }

  private solid(color: string): THREE.MeshStandardMaterial {
    return new THREE.MeshStandardMaterial({
      color,
      roughness: this.studio ? 0.5 : 0.78,
      metalness: 0.0,
      polygonOffset: true,
      polygonOffsetFactor: 1,
      polygonOffsetUnits: 1,
      side: THREE.DoubleSide,
    });
  }

  private transparent(color: string, opacity: number, opts: Partial<THREE.MeshStandardMaterialParameters> = {}): THREE.MeshStandardMaterial {
    return new THREE.MeshStandardMaterial({
      color,
      roughness: 0.9,
      metalness: 0,
      transparent: true,
      opacity,
      depthWrite: false,
      side: THREE.DoubleSide,
      ...opts,
    });
  }

  private lines(color: string, opacity = 1, depthTest = true): THREE.LineBasicMaterial {
    return new THREE.LineBasicMaterial({ color, transparent: opacity < 1 || !depthTest, opacity, depthTest });
  }

  /**
   * Surface/edge materials of a mode; `flat` for 2D nodes, `color` from `color()`,
   * `faceColors`: the geometry has per-vertex colours (colours per face).
   */
  style(mode: Mode, flat: boolean, color: [number, number, number, number] | null, faceColors = false): ModeStyle {
    const key = `${mode}|${flat}|${color?.join(",") ?? ""}|${faceColors}`;
    const c = this.styles.get(key);
    if (c) return c;
    const p = this.palette;
    let s: ModeStyle;
    switch (mode) {
      case "full":
        if (flat) s = { surface: this.transparent(p.flat, 0.18, { polygonOffset: true, polygonOffsetFactor: 1 }), edges: this.lines(p.flat), renderOrder: 1, pickable: true };
        else s = { surface: faceColors ? this.vertexColorMaterial() : this.fullMaterial(color), edges: this.lines(p.edge, this.studio ? 0.35 : 0.9), renderOrder: 0, pickable: true };
        break;
      case "dim":
        s = {
          surface: this.transparent(p.dim, flat ? 0.08 : 0.14, { polygonOffset: true, polygonOffsetFactor: 2, polygonOffsetUnits: 2 }),
          edges: this.lines(p.dimEdge, 0.35),
          renderOrder: 2,
          pickable: true,
        };
        break;
      case "hover":
        s = { surface: this.transparent(p.accent, 0.24, { depthTest: false }), edges: this.lines(p.accent, 0.9, false), renderOrder: 20, pickable: false };
        break;
      case "red":
        s = {
          surface: this.transparent(p.red, 0.38, { polygonOffset: true, polygonOffsetFactor: -1, polygonOffsetUnits: -4 }),
          edges: this.lines(p.red, 0.8),
          renderOrder: 10,
          pickable: false,
        };
        break;
      case "grey":
        s = { surface: this.transparent(p.grey, 0.22), edges: this.lines(p.grey, 0.5), renderOrder: 9, pickable: false };
        break;
      case "blue":
        s = {
          surface: this.transparent(p.blue, 0.24, { polygonOffset: true, polygonOffsetFactor: -1, polygonOffsetUnits: -4 }),
          edges: this.lines(p.blue, 0.6),
          renderOrder: 11,
          pickable: false,
        };
        break;
      case "ghost":
        s = { surface: this.transparent(p.ghost, 0.1, { side: THREE.FrontSide }), edges: this.lines(p.ghost, 0.7), renderOrder: 8, pickable: false };
        break;
      case "outline":
        s = { surface: null, edges: this.lines(p.outline, 0.85), renderOrder: 7, pickable: false };
        break;
      case "picked":
        s = { surface: null, edges: this.lines(p.accent, 1), renderOrder: 15, pickable: false };
        break;
      case "profile":
        // Drawn as thick lines by the viewer (edge segments of the mesh), through the result.
        s = { surface: flat ? this.transparent(p.profile, 0.1, { depthTest: false }) : null, edges: this.lines(p.profile, 1, false), renderOrder: 35, pickable: false };
        break;
      case "keepout":
        // hatched (diagonal stripes in screen space), visible through the model; the viewer
        // adds the dashed outline on top
        s = { surface: this.hatched(p.keepout), edges: null, renderOrder: 12, pickable: false };
        break;
      case "translucent":
        s = { surface: this.transparent(p.model, 0.42, { polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1 }), edges: this.lines(p.edge, 0.45), renderOrder: 6, pickable: true };
        break;
    }
    this.styles.set(key, s);
    return s;
  }

  /** Transparent surface with diagonal stripes (keep-out volumes). */
  private hatched(color: string): THREE.MeshStandardMaterial {
    const m = this.transparent(color, 0.34, { depthTest: false });
    m.onBeforeCompile = (sh) => {
      sh.fragmentShader = sh.fragmentShader.replace(
        "#include <dithering_fragment>",
        "#include <dithering_fragment>\n  gl_FragColor.a *= mod(gl_FragCoord.x + gl_FragCoord.y, 9.0) < 3.0 ? 1.0 : 0.3;",
      );
    };
    m.customProgramCacheKey = () => "keepout-hatch";
    return m;
  }

  private vertexColorMaterial(): THREE.Material {
    let m = this.fullByColor.get("vertex");
    if (!m) {
      const mm = this.solid("#ffffff");
      mm.vertexColors = true;
      m = mm;
      this.fullByColor.set("vertex", m);
    }
    return m;
  }

  private fullMaterial(color: [number, number, number, number] | null): THREE.Material {
    const key = color?.join(",") ?? "";
    let m = this.fullByColor.get(key);
    if (!m) {
      if (color) {
        const c = new THREE.Color(color[0], color[1], color[2]);
        const mm = this.solid(`#${c.getHexString()}`);
        if (color[3] < 1) {
          mm.transparent = true;
          mm.opacity = color[3];
        }
        m = mm;
      } else m = this.solid(this.palette.model);
      this.fullByColor.set(key, m);
    }
    return m;
  }

  /** Thick line material (selected/error edges, highlighted grid lines). */
  fat(color: string, width: number, depthTest = false, dashed = false): LineMaterial {
    const m = new LineMaterial({ color: new THREE.Color(color).getHex(), linewidth: width, depthTest, transparent: true, dashed, dashSize: 1, gapSize: 0.6 });
    this.fatLines.push(m);
    return m;
  }

  setResolution(w: number, h: number): void {
    for (const m of this.fatLines) m.resolution.set(w, h);
  }

  dispose(): void {
    for (const s of this.styles.values()) {
      s.surface?.dispose();
      s.edges?.dispose();
    }
    for (const m of this.fullByColor.values()) m.dispose();
    for (const m of this.fatLines) m.dispose();
  }

  /** Drops per-build thick line materials (they are recreated by every build). */
  releaseFat(): void {
    for (const m of this.fatLines) m.dispose();
    this.fatLines = [];
  }
}
