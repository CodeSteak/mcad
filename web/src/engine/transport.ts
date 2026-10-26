// EngineTransport: how the UI talks to the engine (Plan 7.1). The UI only knows this
// interface; WorkerTransport runs the engine in a Web Worker, TauriTransport
// (tauriTransport.ts) talks to the native engine of the desktop app.

import { t } from "../i18n/index.ts";
import type { FromEngine, ToEngine } from "./protocol";
import { createEngineWorker } from "./workerFactory.ts";

export type EngineListener = (msg: FromEngine) => void;

export interface EngineTransport {
  /** Sends one message to the engine. Messages sent before the engine is ready are queued. */
  send(msg: ToEngine): void;
  /** Subscribes to engine messages; returns the unsubscribe function. */
  onMessage(listener: EngineListener): () => void;
  /** Stops the engine. */
  dispose(): void;
}

/** Envelope worker → main thread. */
export type WorkerOut =
  | { kind: "ready" }
  /** Answers of one `post`/`step` round; `busy`: the worker keeps stepping. */
  | { kind: "out"; msgs: FromEngine[]; busy: boolean }
  /** The engine failed (load error, panic). The worker is unusable. */
  | { kind: "fatal"; message: string };

/** Default watchdog: one kernel call may take this long (like crates/cad-desktop/src/engine.rs). */
export const WATCHDOG_MS = 20 * 60_000;
/** Time allowed for loading smlib.wasm + cad-wasm. */
const STARTUP_MS = 3 * 60_000;

/** "90 s", "20 min". */
function durText(ms: number): string {
  const s = Math.round(ms / 1000);
  return s >= 120 && s % 60 === 0 ? `${s / 60} min` : `${s} s`;
}

export interface WorkerTransportOptions {
  /** Creates the worker (default: `./worker.ts` as module worker). */
  createWorker?: () => Worker;
  /** A step that does not return within this time counts as a hang (default 20 min: large models compute for minutes). */
  watchdogMs?: number;
}

/**
 * Runs the engine in a Web Worker.
 *
 * Watchdog (Plan 7.3): while the worker reports `busy`, every step answers within
 * milliseconds. If no answer arrives for `watchdogMs`, a kernel operation hangs: the worker is
 * terminated and started again (the cache is lost), the UI gets an `Error` with the node that
 * was being evaluated (`Progress.current`). The cache key of that node is blocked: the new
 * worker first gets `BlockKeys` with every key that hung so far (kept over further restarts),
 * then the last `SetContext` and `SetSource` again. The blocked node reports an error (its
 * line turns red) instead of hanging again; an edit that changes the node changes its key and
 * evaluates it normally. Without a node to block, the source is not sent again (the same input
 * would hang again). A `fatal` message from the worker is handled the same way.
 */
export class WorkerTransport implements EngineTransport {
  private worker: Worker | null = null;
  private ready = false;
  private pending: ToEngine[] = [];
  private listeners = new Set<EngineListener>();
  private busy = false;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private lastContext: ToEngine | null = null;
  /** The last `SetLang` (sent first to every new worker). */
  private lastLang: ToEngine | null = null;
  private lastSource: ToEngine | null = null;
  /** Project files as the engine has them (`SetFiles` with the later `SetFile`s folded in). */
  private files: { files: Record<string, string>; main?: string } | null = null;
  /** Cache keys of nodes that hung (sent as `BlockKeys` to every new worker). */
  readonly blockedKeys = new Set<string>();
  private lastProgress: Extract<FromEngine, { type: "Progress" }> | null = null;
  private lastVersion: number | null = null;
  private disposed = false;
  /** Number of worker restarts (watchdog or fatal errors). */
  restarts = 0;

  private readonly createWorker: () => Worker;
  private readonly watchdogMs: number;

  constructor(opts: WorkerTransportOptions = {}) {
    this.createWorker =
      opts.createWorker ?? createEngineWorker;
    this.watchdogMs = opts.watchdogMs ?? WATCHDOG_MS;
    this.start();
  }

  send(msg: ToEngine): void {
    if (this.disposed) return;
    if (msg.type === "SetContext") this.lastContext = msg;
    if (msg.type === "SetLang") this.lastLang = msg;
    if (msg.type === "SetFiles") this.files = { files: { ...msg.files }, main: msg.main };
    if (msg.type === "SetFile") {
      this.files ??= { files: {} };
      if (msg.text === undefined) delete this.files.files[msg.path];
      else this.files.files[msg.path] = msg.text;
    }
    if (msg.type === "SetSource") {
      this.lastVersion = msg.version;
      this.lastSource = msg;
    }
    if (!this.ready || !this.worker) {
      this.pending.push(msg);
      return;
    }
    this.worker.postMessage(msg);
    // Every message may start work; the watchdog covers the answer.
    this.busy = true;
    this.arm();
  }

  onMessage(listener: EngineListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  dispose(): void {
    this.disposed = true;
    this.disarm();
    this.worker?.terminate();
    this.worker = null;
    this.listeners.clear();
  }

  private start(): void {
    this.ready = false;
    const w = this.createWorker();
    this.worker = w;
    w.onmessage = (e: MessageEvent<WorkerOut>) => this.receive(w, e.data);
    w.onerror = (e: ErrorEvent) => {
      e.preventDefault();
      this.restart(`${t("engine.error")}: ${e.message}`);
    };
    // Loading smlib.wasm + cad-wasm can take a while: watch it too.
    this.busy = true;
    this.arm(Math.max(this.watchdogMs, STARTUP_MS));
  }

  private receive(w: Worker, out: WorkerOut): void {
    if (w !== this.worker) return;
    switch (out.kind) {
      case "ready": {
        this.ready = true;
        this.busy = false;
        this.disarm();
        const queued = this.pending;
        this.pending = [];
        for (const m of queued) this.send(m);
        break;
      }
      case "out": {
        this.busy = out.busy;
        if (this.busy) this.arm();
        else this.disarm();
        for (const m of out.msgs) {
          if (m.type === "Progress") this.lastProgress = m;
          if (m.type === "Scene" && m.complete) this.lastProgress = null;
          this.emit(m);
        }
        break;
      }
      case "fatal":
        this.restart(out.message);
        break;
    }
  }

  private emit(m: FromEngine): void {
    for (const l of this.listeners) l(m);
  }

  private arm(ms = this.watchdogMs): void {
    this.disarm();
    this.timer = setTimeout(() => {
      if (this.busy) this.restart(t("engine.hung", { time: durText(ms) }));
    }, ms);
  }

  private disarm(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }

  private restart(reason: string): void {
    if (this.disposed) return;
    this.disarm();
    this.worker?.terminate();
    this.worker = null;
    this.restarts++;
    const cur = this.lastProgress?.current ?? null;
    const where = cur ? ` (${t("status.line", { line: cur.range.line })})` : "";
    const fresh = !!cur?.key && !this.blockedKeys.has(cur.key);
    if (cur?.key) this.blockedKeys.add(cur.key);
    this.emit({
      type: "Error",
      message: `${reason}${where}; ${t("engine.restarted")}${fresh ? `, ${t("engine.nodeBlocked")}` : ""}`,
      version: this.lastProgress?.version ?? this.lastVersion,
      node: cur?.node ?? null,
    });
    this.lastProgress = null;
    const pending: ToEngine[] = [];
    if (this.lastLang) pending.push(this.lastLang);
    if (this.blockedKeys.size) pending.push({ type: "BlockKeys", keys: [...this.blockedKeys] });
    if (this.files) pending.push({ type: "SetFiles", files: { ...this.files.files }, main: this.files.main });

    if (this.lastContext) pending.push(this.lastContext);
    // With a newly blocked node the same source evaluates again (that node reports an error).
    if (fresh && this.lastSource) pending.push(this.lastSource);
    this.pending = pending;
    this.start();
  }
}
