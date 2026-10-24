// Engine client: sends UI messages through an EngineTransport and folds the answers into the
// UI store and the mesh cache.

import type { Edit, ExportFormat, FromEngine, Scene, SketchAnswer, SnapTarget, ToEngine, Value } from "./protocol";
import type { EngineTransport } from "./transport";
import { bridge } from "../state/bridge";
import { store } from "../state/store";
import type { MeshCache } from "../view/meshCache";

type ExportResult = Extract<FromEngine, { type: "ExportResult" }>;

export type EditsListener = (version: number, edits: Edit[]) => void;
export type SnapListener = (handle: number, version: number, targets: SnapTarget[]) => void;
export type SketchListener = (answer: SketchAnswer) => void;

export class EngineClient {
  private requested = new Map<string, number>();
  private exportWaiters: Array<{ resolve: (r: ExportResult) => void; reject: (e: Error) => void }> = [];
  private sentAt = new Map<number, number>();
  private lastContext: string | undefined = undefined;
  private editsListeners: EditsListener[] = [];
  private snapListeners: SnapListener[] = [];
  private sketchListeners: SketchListener[] = [];
  private seq = 0;
  private version = 0;
  private seqs = new WeakMap<Scene, number>();

  constructor(
    private transport: EngineTransport,
    readonly meshes: MeshCache,
  ) {
    transport.onMessage((m) => this.receive(m));
  }

  private send(m: ToEngine): void {
    this.transport.send(m);
  }

  /** Sends a message as it is (drags, defaults, options). */
  post(m: ToEngine): void {
    this.send(m);
  }

  /** `Edits` answers (drags, "Als Standard übernehmen"). */
  onEdits(f: EditsListener): () => void {
    this.editsListeners.push(f);
    return () => (this.editsListeners = this.editsListeners.filter((x) => x !== f));
  }

  onSnapTargets(f: SnapListener): () => void {
    this.snapListeners.push(f);
    return () => (this.snapListeners = this.snapListeners.filter((x) => x !== f));
  }

  /** `Sketch` answers (sketch editor). */
  onSketch(f: SketchListener): () => void {
    this.sketchListeners.push(f);
    return () => (this.sketchListeners = this.sketchListeners.filter((x) => x !== f));
  }

  setDragOptions(stepMm: number, stepDeg: number): void {
    this.send({ type: "SetDragOptions", step_mm: stepMm, step_deg: stepDeg });
  }

  /**
   * Sends the source with a new version. Versions are unique per evaluation request (also
   * across module contexts), so a scene version identifies one evaluation and mesh requests
   * for an older one are dropped by the engine.
   */
  setSource(text: string, editAt?: number): number {
    const version = ++this.version;
    // The mapper follows the document in the editor (the main file or another project file).
    bridge.mapper.mark(version, bridge.doc().length);
    if (editAt !== undefined) bridge.editTimes.set(version, editAt);
    this.sentAt.set(version, performance.now());
    store.setState({ sentVersion: version, pendingSend: false });
    this.send({ type: "SetSource", text, version });
    return version;
  }

  /** Project files besides the main file (sent before the next `SetSource`). */
  setFiles(files: Record<string, string>, main: string): void {
    this.send({ type: "SetFiles", files, main });
  }

  /** One project file changed (`null`: removed). */
  setFile(path: string, text: string | null): void {
    this.send(text === null ? { type: "SetFile", path } : { type: "SetFile", path, text });
  }

  /** Module context with the parameter bar's temporary values (Spec: Parameterleiste). */
  setContext(module: string | null, overrides: Record<string, Value> = {}): void {
    const key = JSON.stringify([module, module === null ? {} : overrides]);
    if (key === this.lastContext) return;
    this.lastContext = key;
    store.setState({ context: module });
    this.send(module === null ? { type: "SetContext", overrides: {} } : { type: "SetContext", module, overrides });
    // Re-send the source with a fresh version: the scene of the new context must not share
    // its version with the previous context's scene.
    if (this.version > 0 && !store.getState().pendingSend) this.setSource(bridge.mainText());
  }

  /** Requests the meshes of `nodes` of the latest complete scene that are not cached yet. */
  ensureMeshes(scene: Scene, nodes: number[]): void {
    const latest = store.getState().complete;
    if (!scene.complete || !latest || latest.version !== scene.version) return;
    const want: number[] = [];
    // Coarse meshes from a drag are replaced by fine ones once the drag is over.
    const coarse = store.getState().drag !== null;
    for (const i of nodes) {
      const n = scene.nodes[i];
      const key = n?.mesh_key;
      if (!key) continue;
      if (this.meshes.has(key) && (coarse || !this.meshes.isCoarse(key))) continue;
      const rk = `${key}|${coarse ? "c" : "f"}`;
      if (this.requested.get(rk) === scene.version) continue;
      this.requested.set(rk, scene.version);
      want.push(i);
    }
    if (want.length) this.send({ type: "RequestMeshes", nodes: want, version: scene.version, coarse });
  }

  /**
   * Export of the model, or of parts (`zip`: one file per part in a ZIP). `colors`: 3MF with
   * one volume per colour (engine default for 3MF), STL as a ZIP with one file per colour.
   */
  exportFile(format: ExportFormat, parts?: string[], zip = false, colors?: boolean): Promise<ExportResult> {
    return new Promise((resolve, reject) => {
      this.exportWaiters.push({ resolve, reject });
      const m: ToEngine = parts ? { type: "Export", format, parts, zip } : { type: "Export", format };
      if (colors !== undefined && m.type === "Export") m.colors = colors;
      this.send(m);
    });
  }

  /** Colour regions of a complete scene (`meshes`: with a mesh per region). */
  requestColorRegions(version: number, meshes: boolean): void {
    this.send({ type: "RequestColorRegions", meshes, version });
  }

  /** Writes a filament slot into the code (`null`: automatic). The answer is an `Edits`. */
  setFilament(color: string, filament: number | null): void {
    this.send(filament === null ? { type: "SetFilament", color } : { type: "SetFilament", color, filament });
  }


  private receive(m: FromEngine): void {
    switch (m.type) {
      case "Diagnostics": {
        if (m.version >= store.getState().diagnostics.version) store.setState({ diagnostics: { version: m.version, items: m.items } });
        break;
      }
      case "Scene": {
        const s = m as Scene;
        this.seqs.set(s, ++this.seq);
        const st = store.getState();
        const patch: Partial<typeof st> = { scene: s };
        if (s.complete) {
          patch.complete = s;
          patch.progress = null;
          patch.engineError = st.engineError && st.engineError.version !== null && st.engineError.version < s.version ? null : st.engineError;
          const t = this.sentAt.get(s.version);
          if (t !== undefined) patch.metrics = { ...st.metrics, evalMs: [...st.metrics.evalMs, performance.now() - t].slice(-200) };
        }
        store.setState(patch);
        this.prune();
        this.updateShown();
        break;
      }
      case "Meshes": {
        for (const it of m.items) this.meshes.put(it);
        store.setState({ meshRevision: store.getState().meshRevision + 1 });
        this.updateShown();
        break;
      }
      case "Progress": {
        store.setState({ progress: { version: m.version, done: m.done, total: m.total, line: m.current?.range.line ?? null } });
        break;
      }
      case "ColorRegions": {
        let meshes = false;
        for (const set of m.items)
          for (const r of set.regions)
            if (r.mesh) {
              this.meshes.put(r.mesh);
              meshes = true;
            }
        const st = store.getState();
        store.setState({ regions: { version: m.version, items: m.items, meshes }, regionsError: null, meshRevision: st.meshRevision + (meshes ? 1 : 0) });
        break;
      }
      case "Error": {
        if (m.message.startsWith("Farbregionen")) store.setState({ regionsError: m.message });
        store.setState({ engineError: { message: m.message, version: m.version, node: m.node, at: performance.now() }, progress: null });
        for (const w of this.exportWaiters.splice(0)) w.reject(new Error(m.message));
        break;
      }
      case "ExportResult": {
        this.exportWaiters.shift()?.resolve(m);
        break;
      }
      case "Edits": {
        for (const f of this.editsListeners) f(m.version, m.edits);
        break;
      }
      case "SnapTargets": {
        for (const f of this.snapListeners) f(m.handle, m.version, m.targets);
        break;
      }
      case "Sketch": {
        for (const f of this.sketchListeners) f(m as SketchAnswer);
        break;
      }
      default:
        break;
    }
  }

  /** The view shows the newest scene (by arrival) whose root mesh is available. */
  private updateShown(): void {
    const st = store.getState();
    const seq = (s: Scene) => this.seqs.get(s) ?? 0;
    for (const s of [st.scene, st.complete]) {
      if (!s) continue;
      if (st.shown && seq(st.shown) >= seq(s)) return;
      const rootKey = s.root !== null ? s.nodes[s.root].mesh_key : null;
      const ready = s.root === null || rootKey === null ? s.complete : this.meshes.has(rootKey);
      if (ready) {
        store.setState({ shown: s });
        return;
      }
    }
  }

  private prune(): void {
    const st = store.getState();
    const vs = [st.scene?.version, st.complete?.version, st.shown?.version, st.diagnostics.version].filter((v): v is number => v !== undefined);
    if (vs.length) {
      const min = Math.min(...vs);
      bridge.mapper.prune(min);
      for (const v of this.sentAt.keys()) if (v < min) this.sentAt.delete(v);
    }
  }
}
