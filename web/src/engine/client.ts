// Engine client: sends UI messages through an EngineTransport and folds the answers into the
// UI store and the mesh cache.

import type { ExportFormat, FromEngine, Scene, ToEngine } from "./protocol";
import type { EngineTransport } from "./transport";
import { bridge } from "../state/bridge";
import { store } from "../state/store";
import type { MeshCache } from "../view/meshCache";

type ExportResult = Extract<FromEngine, { type: "ExportResult" }>;

export class EngineClient {
  private requested = new Map<string, number>();
  private exportWaiters: Array<{ resolve: (r: ExportResult) => void; reject: (e: Error) => void }> = [];
  private sentAt = new Map<number, number>();
  private lastContext: string | null | undefined = undefined;
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

  /**
   * Sends the source with a new version. Versions are unique per evaluation request (also
   * across module contexts), so a scene version identifies one evaluation and mesh requests
   * for an older one are dropped by the engine.
   */
  setSource(text: string, editAt?: number): number {
    const version = ++this.version;
    bridge.mapper.mark(version, text.length);
    if (editAt !== undefined) bridge.editTimes.set(version, editAt);
    this.sentAt.set(version, performance.now());
    store.setState({ sentVersion: version, pendingSend: false });
    this.send({ type: "SetSource", text, version });
    return version;
  }

  setContext(module: string | null): void {
    if (module === this.lastContext) return;
    this.lastContext = module;
    store.setState({ context: module });
    this.send(module === null ? { type: "SetContext", overrides: {} } : { type: "SetContext", module, overrides: {} });
    // Re-send the source with a fresh version: the scene of the new context must not share
    // its version with the previous context's scene.
    if (this.version > 0 && !store.getState().pendingSend) this.setSource(bridge.text());
  }

  /** Requests the meshes of `nodes` of the latest complete scene that are not cached yet. */
  ensureMeshes(scene: Scene, nodes: number[]): void {
    const latest = store.getState().complete;
    if (!scene.complete || !latest || latest.version !== scene.version) return;
    const want: number[] = [];
    for (const i of nodes) {
      const n = scene.nodes[i];
      const key = n?.mesh_key;
      if (!key || this.meshes.has(key)) continue;
      if (this.requested.get(key) === scene.version) continue;
      this.requested.set(key, scene.version);
      want.push(i);
    }
    if (want.length) this.send({ type: "RequestMeshes", nodes: want, version: scene.version });
  }

  exportFile(format: ExportFormat): Promise<ExportResult> {
    return new Promise((resolve, reject) => {
      this.exportWaiters.push({ resolve, reject });
      this.send({ type: "Export", format });
    });
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
      case "Error": {
        store.setState({ engineError: { message: m.message, version: m.version, node: m.node, at: performance.now() }, progress: null });
        for (const w of this.exportWaiters.splice(0)) w.reject(new Error(m.message));
        break;
      }
      case "ExportResult": {
        this.exportWaiters.shift()?.resolve(m);
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
