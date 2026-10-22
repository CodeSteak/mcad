/// <reference lib="webworker" />
// Engine worker: loads smlib.wasm (SMLib kernel, Emscripten) and cad-wasm (Rust session),
// then runs the loop
//
//   messages from the UI → Session.post → answers
//   while busy: Session.step(~16 ms) → answers, then yield to the event loop (MessageChannel)
//
// Yielding between steps lets new messages in: a new SetSource aborts the running kernel job
// (finished nodes stay cached). Meshes and export bytes are transferred, not copied.
// Envelope to the main thread: `WorkerOut` (transport.ts).

import initCad, { Session } from "../wasm-pkg/cad_wasm.js";
import cadWasmUrl from "../wasm-pkg/cad_wasm_bg.wasm?url";
import createSmlib from "../wasm-pkg/smlib.js";
import smlibWasmUrl from "../wasm-pkg/smlib.wasm?url";
import { initSmlib } from "../wasm-pkg/smlib-bridge.js";
import type { FromEngine, ToEngine } from "./protocol";
import type { WorkerOut } from "./transport";

declare const self: DedicatedWorkerGlobalScope;

/** Time budget of one step in ms. */
const STEP_BUDGET_MS = 16;

let session: Session | null = null;
let dead = false;
const queue: ToEngine[] = [];
let scheduled = false;

const channel = new MessageChannel();
channel.port1.onmessage = () => {
  scheduled = false;
  tick();
};

function schedule(): void {
  if (!scheduled && !dead) {
    scheduled = true;
    channel.port2.postMessage(null);
  }
}

function send(out: WorkerOut, transfer: Transferable[] = []): void {
  self.postMessage(out, transfer);
}

function fatal(err: unknown): void {
  dead = true;
  session = null;
  const message = err instanceof Error ? `${err.message}` : String(err);
  send({ kind: "fatal", message });
}

/** Buffers of typed arrays in the answers (meshes, export bytes). */
function transferables(msgs: FromEngine[]): Transferable[] {
  const t: Transferable[] = [];
  for (const m of msgs) {
    if (m.type === "Meshes") {
      for (const it of m.items) {
        for (const a of [it.positions, it.normals, it.indices, it.tri_tags, it.tri_faces, it.edge_segments, it.edge_ids]) {
          t.push(a.buffer as ArrayBuffer);
        }
      }
    } else if (m.type === "ExportResult") {
      t.push(m.bytes.buffer as ArrayBuffer);
    }
  }
  return t;
}

/** Drops a SetSource that is directly followed by another SetSource (it would be aborted). */
function drain(): ToEngine[] {
  const msgs = queue.splice(0);
  return msgs.filter((m, i) => !(m.type === "SetSource" && msgs[i + 1]?.type === "SetSource"));
}

function tick(): void {
  if (!session || dead) return;
  const out: FromEngine[] = [];
  try {
    for (const m of drain()) {
      out.push(...(session.post(m) as FromEngine[]));
    }
    if (session.busy()) {
      out.push(...(session.step(STEP_BUDGET_MS) as FromEngine[]));
    }
  } catch (err) {
    // A Rust panic aborts the Wasm instance: it cannot be used any more.
    if (out.length) send({ kind: "out", msgs: out, busy: false }, transferables(out));
    fatal(err);
    return;
  }
  const busy = session.busy() || queue.length > 0;
  send({ kind: "out", msgs: out, busy }, transferables(out));
  if (busy) schedule();
}

self.onmessage = (e: MessageEvent<ToEngine>) => {
  queue.push(e.data);
  if (session) schedule();
};

async function init(): Promise<void> {
  await initSmlib(createSmlib, { locateFile: (p: string) => (p.endsWith(".wasm") ? smlibWasmUrl : p) });
  await initCad({ module_or_path: cadWasmUrl });
  session = new Session();
  send({ kind: "ready" });
  if (queue.length) schedule();
}

init().catch(fatal);
