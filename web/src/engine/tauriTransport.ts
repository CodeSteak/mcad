// TauriTransport: the engine runs natively in the desktop app (Plan 7.1). Messages to the
// engine go as one `engine_post` command per microtask (in order, batched); answers come over
// a Tauri 2 channel as binary frames (crates/cad-desktop/src/wire.rs):
//
//   [u32 LE header length][u32 LE data length][header JSON (space padded to 4)][data]
//   header = { v, id, kind, method: "out" | "host" | "ready", body }
//   out:  body = { msgs: FromEngine[], busy, seen } with typed arrays as { "$b": [offset, len, "f32"|"u32"|"u8"] }
//   host: body = { ready, busy, restarts, blocked_keys }
//
// Mesh arrays become views on the received ArrayBuffer (no JSON number arrays, no copy).
// The watchdog lives in the app (crates/cad-desktop/src/engine.rs): a hanging kernel call
// replaces the engine process and blocks the node's key, like WorkerTransport does.

import { Channel, invoke } from "@tauri-apps/api/core";
import { t } from "../i18n";
import type { FromEngine, ToEngine } from "./protocol";
import type { EngineListener, EngineTransport } from "./transport";

export interface DecodedFrame {
  method: string;
  kind: string;
  body: unknown;
}

type Blob = { $b: [number, number, "f32" | "u32" | "u8"] };

function isBlob(v: unknown): v is Blob {
  return typeof v === "object" && v !== null && Array.isArray((v as Blob).$b) && (v as Blob).$b.length === 3;
}

/** Decodes one frame; blobs become typed array views on `buf`. */
export function decodeFrame(buf: ArrayBuffer): DecodedFrame {
  const dv = new DataView(buf);
  const hl = dv.getUint32(0, true);
  const dl = dv.getUint32(4, true);
  const header = JSON.parse(new TextDecoder().decode(new Uint8Array(buf, 8, hl))) as { v: number; kind: string; method: string; body: unknown };
  const base = 8 + hl;
  if (base + dl > buf.byteLength) throw new Error(t("engine.frameIncomplete"));
  const fix = (v: unknown): unknown => {
    if (Array.isArray(v)) {
      for (let i = 0; i < v.length; i++) v[i] = fix(v[i]);
      return v;
    }
    if (isBlob(v)) {
      const [off, len, kind] = v.$b;
      const at = base + off;
      if (kind === "f32") return new Float32Array(buf, at, len);
      if (kind === "u32") return new Uint32Array(buf, at, len);
      return new Uint8Array(buf, at, len);
    }
    if (typeof v === "object" && v !== null) {
      const o = v as Record<string, unknown>;
      for (const k of Object.keys(o)) o[k] = fix(o[k]);
    }
    return v;
  };
  return { method: header.method, kind: header.kind, body: fix(header.body) };
}

/** The parts of @tauri-apps/api the transport needs (injected for tests). */
export interface TauriApi {
  invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown>;
  channel(onmessage: (data: ArrayBuffer) => void): unknown;
}

export class TauriTransport implements EngineTransport {
  private listeners = new Set<EngineListener>();
  private queue: unknown[] = [];
  private flushing = false;
  private scheduled = false;
  private disposed = false;
  /** Engine restarts by the watchdog (from the app). */
  restarts = 0;
  /** Cache keys of nodes that hung. */
  readonly blockedKeys = new Set<string>();
  /** The engine is busy (last batch). */
  busy = false;
  /** Bytes received over the channel (test hook, report). */
  bytes = 0;

  constructor(private api: TauriApi) {
    const ch = api.channel((data) => this.receive(data));
    void api.invoke("engine_start", { channel: ch });
  }

  send(msg: ToEngine): void {
    if (this.disposed) return;
    this.queue.push(msg);
    if (!this.scheduled) {
      this.scheduled = true;
      queueMicrotask(() => {
        this.scheduled = false;
        void this.flush();
      });
    }
  }

  /** Test aid for the watchdog: blocks the engine like a hanging kernel call. */
  debugHang(ms: number): void {
    this.queue.push({ type: "__debugHang", ms });
    if (!this.scheduled) {
      this.scheduled = true;
      queueMicrotask(() => {
        this.scheduled = false;
        void this.flush();
      });
    }
  }

  private async flush(): Promise<void> {
    if (this.flushing || !this.queue.length) return;
    this.flushing = true;
    try {
      while (this.queue.length) {
        const msgs = this.queue.splice(0);
        await this.api.invoke("engine_post", { msgs });
      }
    } catch (e) {
      this.emit({ type: "Error", message: t("engine.unreachable", { error: String((e as Error).message ?? e) }), version: null, node: null });
    } finally {
      this.flushing = false;
    }
  }

  onMessage(listener: EngineListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  dispose(): void {
    this.disposed = true;
    this.listeners.clear();
  }

  private emit(m: FromEngine): void {
    for (const l of this.listeners) l(m);
  }

  /** Handles one frame (public for tests). */
  receive(data: ArrayBuffer): void {
    if (this.disposed) return;
    this.bytes += data.byteLength;
    const f = decodeFrame(data);
    if (f.method === "host") {
      const b = f.body as { restarts: number; blocked_keys: string[]; busy: boolean };
      this.restarts = b.restarts;
      for (const k of b.blocked_keys) this.blockedKeys.add(k);
      return;
    }
    if (f.method !== "out") return;
    const b = f.body as { msgs: FromEngine[]; busy: boolean };
    this.busy = b.busy;
    for (const m of b.msgs) this.emit(m);
  }
}

/** The transport over the real Tauri API. */
export function createTauriTransport(): TauriTransport {
  return new TauriTransport({
    invoke: (cmd, args) => invoke(cmd, args),
    channel: (onmessage) => {
      const ch = new Channel<ArrayBuffer>();
      ch.onmessage = onmessage;
      return ch;
    },
  });
}
