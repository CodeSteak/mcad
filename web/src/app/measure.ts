// Measure mode (Plan 18). The engine does the work – what lies under the mouse (vertex, edge
// midpoint, circle centre, edge, face; in the section the contour), the measurement on the
// exact B-rep, the texts and the `mcad measure` arguments; this controller only forwards the
// mouse (ray, the face the view's raycast hit, Alt, clicks) and keeps the answers:
//
//   mouse move → `Measure { ray, hit }` (at most one in flight, the latest position wins)
//   click      → `Measure { ray, hit, click }` → picks kept, or a measurement (listed, drawn)
//   Esc        → drops the picks; again: leaves the mode
//
// Left-dragging still orbits (a click is a press without movement, like picking faces).

import type { EngineClient } from "../engine/client";
import type { MeasureAnswer, MeasureHit, MeasureKind, MeasureRequest, Ray, SectionPlaneRef } from "../engine/protocol";
import { MEASURE_KINDS, fitKind, kindAvailable, listText, measureCli, measureComment } from "../logic/measure";
import { effectiveNormal } from "../logic/section";
import { pixelSize } from "../logic/sketchEditor";
import { absPath, IS_TAURI } from "../platform/desktop";
import { project } from "../state/project";
import { type MeasureState, store } from "../state/store";
import { copyText } from "./clipboard";

/** What the controller needs from the 3D view. */
export interface MeasureView {
  ray(clientX: number, clientY: number): Ray;
  /** The face under a client position (raycast on the displayed meshes, section respected). */
  measureHit(clientX: number, clientY: number): MeasureHit | null;
  fov(): { deg: number; height: number };
}

interface Pending {
  x: number;
  y: number;
  alt: boolean;
}

export class MeasureController {
  private view: MeasureView | null = null;
  private seq = 0;
  private nextId = 1;
  private inflight: { id: number; click: boolean; at: number } | null = null;
  private clicks: Pending[] = [];
  private move: Pending | null = null;
  private infoWanted = false;
  private listeners: Array<[string, EventListener]> = [];
  /** Answers (tests, report): count and round trips in ms. */
  answers = 0;
  rtt: Array<{ ms: number; click: boolean }> = [];

  constructor(private client: EngineClient) {
    client.onMeasure((a) => this.onAnswer(a));
    // the model changed: picks of the old version are dropped
    store.subscribe((s, p) => {
      const m = s.measure;
      if (!m || s.complete === p.complete || !s.complete) return;
      if (m.version !== null && m.version !== s.complete.version && (m.picks.length || m.hover)) {
        const msg = m.picks.length ? "Modell geändert – Auswahl verworfen" : null;
        this.patch({ picks: [], hover: null, version: s.complete.version, message: msg });
        if (msg) store.setState({ notice: msg });
      }
      this.request(null, false, true);
    });
    store.subscribe((s, p) => {
      const m = s.measure;
      if (!m || s.section === p.section) return;
      if (!s.section.on && m.inSection) {
        // the section view was switched off: measure on the model again
        this.clicks = [];
        this.patch({ inSection: false, kind: fitKind(m.kind, false), picks: [], hover: null, section: null });
        this.request(null, false, true);
      } else if (m.inSection) {
        // another plane: picks of the old contour are gone
        this.clicks = [];
        this.patch({ picks: [], hover: null, section: null });
        this.request(null, false, true);
      }
    });
  }

  setView(v: MeasureView | null): void {
    this.view = v;
  }

  get active(): boolean {
    return store.getState().measure !== null;
  }

  private state(): MeasureState | null {
    return store.getState().measure;
  }

  private patch(p: Partial<MeasureState>): void {
    const s = this.state();
    if (s) store.setState({ measure: { ...s, ...p } });
  }

  /** Enters the measure mode (the caller ends the sketch editor). */
  enter(kind: MeasureKind = "distance", inSection?: boolean): void {
    const st = store.getState();
    const sec = inSection ?? (st.measure?.inSection ?? false);
    if (this.active) {
      this.setKind(kind);
      return;
    }
    this.clicks = [];
    this.move = null;
    const on = sec && st.section.on;
    store.setState({
      measure: { kind: fitKind(kind, on), inSection: on, picks: [], hover: null, prompt: "", message: null, section: null, version: st.complete?.version ?? null },
    });
    this.listen("keydown", (e) => this.onKey(e as KeyboardEvent));
    this.request(null, false, true);
  }

  exit(): void {
    if (!this.active) return;
    for (const [t, f] of this.listeners) window.removeEventListener(t, f, true);
    this.listeners = [];
    this.clicks = [];
    this.move = null;
    store.setState({ measure: null });
  }

  setKind(kind: MeasureKind): void {
    const s = this.state();
    if (!s) return this.enter(kind);
    this.clicks = [];
    this.patch({ kind: fitKind(kind, s.inSection), picks: [], hover: null, message: null });
    this.request(null, false, true);
  }

  /** "Im Schnitt": measure in the section plane (switches the section view on). */
  setInSection(on: boolean, turnOnSection: () => void): void {
    const s = this.state();
    if (!s) return;
    if (on && !store.getState().section.on) turnOnSection();
    this.clicks = [];
    this.patch({ inSection: on, kind: fitKind(s.kind, on), picks: [], hover: null, message: null, section: null });
    this.request(null, false, true);
  }

  /** Mouse moved over the view. */
  hover(x: number, y: number, alt: boolean): void {
    if (!this.active) return;
    this.move = { x, y, alt };
    this.pump();
  }

  /** Mouse click (press and release without moving). */
  click(x: number, y: number, alt: boolean): void {
    if (!this.active) return;
    this.clicks.push({ x, y, alt });
    this.move = null;
    this.pump();
  }

  /** Mouse left the view: no element under it. */
  leave(): void {
    this.move = null;
    if (this.state()?.hover) this.patch({ hover: null });
  }

  /** Esc: drop the picks; without picks leave the mode. */
  escape(): void {
    const s = this.state();
    if (!s) return;
    if (s.picks.length) {
      this.clicks = [];
      this.patch({ picks: [], message: null });
      this.request(null, false, true);
    } else this.exit();
  }

  private plane(): SectionPlaneRef | null {
    const sec = store.getState().section;
    return sec.on ? { point: sec.point, normal: effectiveNormal(sec) } : null;
  }

  private request(p: Pending | null, click: boolean, info: boolean): void {
    if (info) this.infoWanted = true;
    if (this.inflight) return;
    const s = this.state();
    if (!s) return;
    const id = ++this.seq;
    const st = store.getState();
    const f = this.view?.fov() ?? { deg: 35, height: 800 };
    const plane = this.plane();
    const r: MeasureRequest = {
      id,
      kind: s.kind,
      picks: s.picks,
      pixel: pixelSize(f.deg, f.height),
      snap_px: st.settings.snapPx,
      free: p?.alt ?? false,
      click,
    };
    if (p && this.view) {
      r.ray = this.view.ray(p.x, p.y);
      const h = this.view.measureHit(p.x, p.y);
      if (h) r.hit = h;
    }
    if (plane && s.inSection) r.section = plane;
    else if (plane) r.clip = plane;
    if (!p) this.infoWanted = false;
    this.inflight = { id, click, at: performance.now() };
    this.client.post({ type: "Measure", ...r });
  }

  private pump(): void {
    if (this.inflight || !this.active) return;
    const c = this.clicks.shift();
    if (c) return this.request(c, true, false);
    const m = this.move;
    if (m) {
      this.move = null;
      this.request(m, false, false);
    } else if (this.infoWanted) this.request(null, false, true);
  }

  private onAnswer(a: MeasureAnswer): void {
    const f = this.inflight;
    if (!f || f.id !== a.id) return;
    this.inflight = null;
    this.answers++;
    this.rtt = [...this.rtt, { ms: performance.now() - f.at, click: f.click }].slice(-400);
    const s = this.state();
    if (s) {
      if (f.click) {
        this.patch({ picks: a.picks, hover: null, prompt: a.prompt, message: a.message, section: a.section ?? s.section, version: a.version });
        if (a.result) {
          const st = store.getState();
          const file = IS_TAURI && project.root ? absPath(project.root, project.main) : project.main;
          const id = this.nextId++;
          store.setState({
            measurements: [...st.measurements, { id, m: a.result, version: a.version, file, module: st.shown?.context.module ?? null }],
            measureSelected: id,
          });
        }
        if (a.message) store.setState({ notice: a.message });
      } else {
        this.patch({ hover: a.hover, prompt: a.prompt, message: null, section: a.section ?? (s.inSection ? s.section : null), version: a.version });
      }
    }
    this.pump();
  }

  // ---------------------------------------------------------------- the list

  remove(id: number): void {
    const st = store.getState();
    store.setState({ measurements: st.measurements.filter((x) => x.id !== id), measureSelected: st.measureSelected === id ? null : st.measureSelected });
  }

  clear(): void {
    store.setState({ measurements: [], measureSelected: null });
  }

  select(id: number | null): void {
    store.setState({ measureSelected: id });
  }

  /** Copies a measurement: as text, as a code comment or as a `mcad measure` call. */
  async copy(id: number, how: "text" | "comment" | "cli"): Promise<string | null> {
    const e = store.getState().measurements.find((x) => x.id === id);
    if (!e) return null;
    const text = how === "text" ? e.m.text : how === "comment" ? measureComment(e.m) : measureCli(e.m, e.file);
    const ok = await copyText(text);
    const what = how === "text" ? "Messung" : how === "comment" ? "Kommentar" : "CLI-Aufruf";
    store.setState({ notice: ok ? `Kopiert (${what}): ${text.split("\n")[0]}` : `${what}: ${text.split("\n")[0]}`, lastCli: how === "cli" ? text : store.getState().lastCli });
    return text;
  }

  /** Copies all measurements as text. */
  async copyAll(): Promise<string> {
    const text = listText(store.getState().measurements.map((x) => x.m));
    const ok = await copyText(text);
    store.setState({ notice: ok ? `Kopiert: ${store.getState().measurements.length} Messungen` : "Kopieren nicht möglich" });
    return text;
  }

  private listen(type: string, f: EventListener): void {
    window.addEventListener(type, f, true);
    this.listeners.push([type, f]);
  }

  private onKey(e: KeyboardEvent): void {
    if (!this.active) return;
    // keys in menus and dialogs belong to them (Esc closes the menu, not the mode)
    if ((e.target as HTMLElement | null)?.closest?.(".ui-menu, [role=dialog]")) return;
    const inEditor = (e.target as HTMLElement | null)?.closest?.(".cm-editor") != null;
    const inField = (e.target as HTMLElement | null)?.closest?.("input, select, textarea") != null;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      this.escape();
      return;
    }
    if (inEditor || inField) return;
    const sel = store.getState().measureSelected;
    if ((e.key === "Delete" || e.key === "Backspace") && sel !== null) {
      e.preventDefault();
      this.remove(sel);
      return;
    }
    // letters choose the kind (D, R, W, L, F, K, T)
    const s = this.state();
    if (!s || e.ctrlKey || e.metaKey || e.altKey) return;
    const k = MEASURE_KINDS.find((x) => x.key === e.key.toUpperCase());
    if (k && kindAvailable(k.kind, s.inSection)) {
      e.preventDefault();
      this.setKind(k.kind);
    }
  }
}
