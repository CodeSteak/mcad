// UI state (Zustand). The engine owns the model; the UI only keeps display data: the latest
// scenes, diagnostics, cursor and hover positions, settings.

import { createStore } from "zustand/vanilla";
import { useStore } from "zustand";
import type { Diagnostic, Scene } from "../engine/protocol";
import type { ParamOverrides } from "../logic/params";

export type Theme = "light" | "dark";

export interface Progress {
  version: number;
  done: number;
  total: number;
  line: number | null;
}

export interface EngineError {
  message: string;
  version: number | null;
  node: number | null;
  at: number;
}

export interface ViewHover {
  /** Lines (current document, 1-based) of the hovered face. */
  lines: number[];
  nodes: number[];
  sceneVersion: number;
}

export interface Metrics {
  /** ms from navigation start until the first scene was drawn. */
  firstSceneMs: number | null;
  /** ms from an edit until the scene of that edit was drawn. */
  editToScene: number[];
  /** ms from SetSource until the complete scene arrived. */
  evalMs: number[];
  /** Handle drags: ms from the Drag message until the resulting scene was drawn. */
  dragToScene: Array<{ ms: number; coarse: boolean }>;
  /** ms from releasing a handle until the fine root mesh was drawn. */
  dragEndToFine: number[];
  /** Frames per second while dragging (one entry per drag). */
  dragFps: Array<{ fps: number; frames: number; ms: number; maxGapMs: number; sent: number; dropped: number }>;
}

/** Settings (Einstellungen), kept in localStorage. */
export interface Settings {
  /** Rounding step of free drags in mm. */
  stepMm: number;
  /** Rounding step of angles in degrees. */
  stepDeg: number;
  /** Snap radius in screen pixels. */
  snapPx: number;
}

export const DEFAULT_SETTINGS: Settings = { stepMm: 1, stepDeg: 15, snapPx: 14 };

export interface DragInfo {
  handle: number;
  /** Text of the snap target under the mouse (`g[rechts, hinten, deckel]`). */
  snap: string | null;
  alt: boolean;
  /** Live edits applied so far. */
  edits: number;
}

export interface CadState {
  fileName: string;
  /** Number of editor document changes. */
  docVersion: number;
  /** Version of the last source sent to the engine (unique per evaluation request). */
  sentVersion: number;
  /** An edit is waiting to be sent. */
  pendingSend: boolean;
  /** First source version of the currently loaded file (camera fit). */
  loadVersion: number;
  /** Incremented when the editor document changes (mapping of old scenes changes). */
  docRevision: number;
  /** Latest scene (preliminary or complete). */
  scene: Scene | null;
  /** Latest complete scene. */
  complete: Scene | null;
  /** Scene drawn in the view (root mesh available). */
  shown: Scene | null;
  diagnostics: { version: number; items: Diagnostic[] };
  progress: Progress | null;
  engineError: EngineError | null;
  engineReady: boolean;
  /** Module requested with SetContext. */
  context: string | null;
  cursor: number;
  editorHover: number | null;
  viewHover: ViewHover | null;
  allGrids: boolean;
  theme: Theme;
  /** Incremented when meshes arrive. */
  meshRevision: number;
  metrics: Metrics;
  /** Last status message of a file action. */
  notice: string | null;
  settings: Settings;
  /** Temporary parameter values per module (parameter bar). */
  paramOverrides: ParamOverrides;
  /** Handle drag in progress. */
  drag: DragInfo | null;
  /** Handle under the mouse in the view (id). */
  hoverHandle: number | null;
}

function initialSettings(): Settings {
  try {
    const raw = localStorage.getItem("cad-settings");
    if (raw) {
      const s = JSON.parse(raw) as Partial<Settings>;
      const ok = (v: unknown, d: number) => (typeof v === "number" && Number.isFinite(v) && v >= 0 ? v : d);
      return { stepMm: ok(s.stepMm, DEFAULT_SETTINGS.stepMm), stepDeg: ok(s.stepDeg, DEFAULT_SETTINGS.stepDeg), snapPx: ok(s.snapPx, DEFAULT_SETTINGS.snapPx) };
    }
  } catch {
    /* no storage */
  }
  return { ...DEFAULT_SETTINGS };
}

export function saveSettings(s: Settings): void {
  try {
    localStorage.setItem("cad-settings", JSON.stringify(s));
  } catch {
    /* no storage */
  }
}

function initialTheme(): Theme {
  try {
    const t = localStorage.getItem("cad-theme");
    if (t === "light" || t === "dark") return t;
  } catch {
    /* no storage */
  }
  return typeof matchMedia !== "undefined" && matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export const store = createStore<CadState>(() => ({
  fileName: "unbenannt.scad",
  docVersion: 0,
  sentVersion: 0,
  pendingSend: false,
  loadVersion: 0,
  docRevision: 0,
  scene: null,
  complete: null,
  shown: null,
  diagnostics: { version: 0, items: [] },
  progress: null,
  engineError: null,
  engineReady: false,
  context: null,
  cursor: 0,
  editorHover: null,
  viewHover: null,
  allGrids: false,
  theme: initialTheme(),
  meshRevision: 0,
  metrics: { firstSceneMs: null, editToScene: [], evalMs: [], dragToScene: [], dragEndToFine: [], dragFps: [] },
  notice: null,
  settings: initialSettings(),
  paramOverrides: {},
  drag: null,
  hoverHandle: null,
}));

export function useCad<T>(sel: (s: CadState) => T): T {
  return useStore(store, sel);
}
