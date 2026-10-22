// EngineTransport: how the UI talks to the engine (Plan 7.1). The UI only knows this
// interface; WorkerTransport runs the engine in a Web Worker, a later TauriTransport will use
// IPC.

import type { FromEngine, ToEngine } from "./protocol";

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

export interface WorkerTransportOptions {
  /** Creates the worker (default: `./worker.ts` as module worker). */
  createWorker?: () => Worker;
  /** A step that does not return within this time counts as a hang (default 15 s). */
  watchdogMs?: number;
}

/**
 * Runs the engine in a Web Worker.
 *
 * Watchdog: while the worker reports `busy`, every step answers within milliseconds. If no
 * answer arrives for `watchdogMs`, a kernel operation hangs: the worker is terminated and
 * started again (the cache is lost), the UI gets an `Error` with the node that was being
 * evaluated (`Progress.current`), and the last `SetContext` is sent again. The source is
 * *not* sent again, so the same input does not hang again; the next `SetSource` (an edit)
 * evaluates normally. A `fatal` message from the worker is handled the same way.
 */
export class WorkerTransport implements EngineTransport {
  private worker: Worker | null = null;
  private ready = false;
  private pending: ToEngine[] = [];
  private listeners = new Set<EngineListener>();
  private busy = false;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private lastContext: ToEngine | null = null;
  private lastProgress: Extract<FromEngine, { type: "Progress" }> | null = null;
  private lastVersion: number | null = null;
  private disposed = false;
  /** Number of worker restarts (watchdog or fatal errors). */
  restarts = 0;

  private readonly createWorker: () => Worker;
  private readonly watchdogMs: number;

  constructor(opts: WorkerTransportOptions = {}) {
    this.createWorker =
      opts.createWorker ?? (() => new Worker(new URL("./worker.ts", import.meta.url), { type: "module" }));
    this.watchdogMs = opts.watchdogMs ?? 15_000;
    this.start();
  }

  send(msg: ToEngine): void {
    if (this.disposed) return;
    if (msg.type === "SetContext") this.lastContext = msg;
    if (msg.type === "SetSource") this.lastVersion = msg.version;
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
      this.restart(`Engine-Fehler: ${e.message}`);
    };
    // Loading smlib.wasm + cad-wasm can take a while: watch it too.
    this.busy = true;
    this.arm(Math.max(this.watchdogMs, 60_000));
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
      if (this.busy) this.restart(`Engine reagiert seit ${Math.round(ms / 1000)} s nicht (Kernel hängt)`);
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
    const where = cur ? ` (Zeile ${cur.range.line})` : "";
    this.emit({
      type: "Error",
      message: `${reason}${where}; Engine neu gestartet, Cache verworfen`,
      version: this.lastProgress?.version ?? this.lastVersion,
      node: cur?.node ?? null,
    });
    this.lastProgress = null;
    this.pending = this.lastContext ? [this.lastContext] : [];
    this.start();
  }
}
