// Node test of WorkerTransport (queueing, message fan-out, watchdog restart) with a fake
// worker. Needs Node >= 22.18 (TypeScript type stripping).
//
//   node web/scripts/test-transport.mjs

import assert from "node:assert/strict";
import { WorkerTransport } from "../src/engine/transport.ts";

class FakeWorker {
  static all = [];
  constructor() {
    this.sent = [];
    this.terminated = false;
    this.onmessage = null;
    this.onerror = null;
    FakeWorker.all.push(this);
  }
  postMessage(m) {
    this.sent.push(m);
  }
  terminate() {
    this.terminated = true;
  }
  /** Simulates a message from the worker. */
  out(data) {
    this.onmessage?.({ data });
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const t = new WorkerTransport({ createWorker: () => new FakeWorker(), watchdogMs: 50 });
const got = [];
t.onMessage((m) => got.push(m));

// queued until ready
t.send({ type: "SetContext", module: "gehaeuse", overrides: {} });
t.send({ type: "SetSource", text: "cube(1);", version: 7 });
const w1 = FakeWorker.all[0];
assert.equal(w1.sent.length, 0);
w1.out({ kind: "ready" });
assert.deepEqual(
  w1.sent.map((m) => m.type),
  ["SetContext", "SetSource"],
);

// answers are fanned out; busy keeps the watchdog armed
w1.out({
  kind: "out",
  busy: true,
  msgs: [{ type: "Progress", version: 7, done: 1, total: 3, current: { node: 2, stmt: 0, range: { line: 4 } } }],
});
assert.equal(got.at(-1).type, "Progress");
await sleep(20);
w1.out({ kind: "out", busy: true, msgs: [] }); // heartbeat within the limit
await sleep(30);
assert.equal(t.restarts, 0);

// hang: no answer for > 50 ms → restart, error with the node, context re-sent
await sleep(80);
assert.equal(t.restarts, 1);
assert.ok(w1.terminated);
const err = got.find((m) => m.type === "Error");
assert.ok(err, "error message");
assert.equal(err.node, 2);
assert.equal(err.version, 7);
assert.match(err.message, /Zeile 4/);
const w2 = FakeWorker.all[1];
w2.out({ kind: "ready" });
assert.deepEqual(
  w2.sent.map((m) => m.type),
  ["SetContext"],
  "only the context is re-sent, not the source",
);
w2.out({ kind: "out", busy: false, msgs: [] });

// idle: no restart even without messages
await sleep(80);
assert.equal(t.restarts, 1);

// fatal error from the worker → restart
w2.out({ kind: "fatal", message: "RuntimeError: unreachable" });
assert.equal(t.restarts, 2);
assert.match(got.at(-1).message, /unreachable/);
FakeWorker.all[2].out({ kind: "ready" });

// messages of a terminated worker are ignored
w1.out({ kind: "out", busy: false, msgs: [{ type: "Error", message: "stale", version: null, node: null }] });
assert.notEqual(got.at(-1).message, "stale");

t.dispose();
console.log("test-transport: ok");
