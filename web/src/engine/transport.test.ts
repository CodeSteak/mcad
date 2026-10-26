import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FromEngine, ToEngine } from "./protocol";
import { WorkerTransport, type WorkerOut } from "./transport";

/** Fake worker: records posted messages; the test answers through `out`. */
class FakeWorker {
  posted: ToEngine[] = [];
  terminated = false;
  onmessage: ((e: MessageEvent<WorkerOut>) => void) | null = null;
  onerror: ((e: ErrorEvent) => void) | null = null;
  postMessage(m: ToEngine) {
    this.posted.push(m);
  }
  terminate() {
    this.terminated = true;
  }
  out(o: WorkerOut) {
    this.onmessage?.({ data: o } as MessageEvent<WorkerOut>);
  }
}

const progress = (key: string, line: number): FromEngine => ({
  type: "Progress",
  version: 1,
  done: 0,
  total: 3,
  current: { node: 4, label: "fillet", range: { from: 0, to: 1, line, col: 1, end_line: line, end_col: 2 }, key } as never,
});

describe("Watchdog: hängende Knoten sperren", () => {
  let workers: FakeWorker[];
  let t: WorkerTransport;
  let got: FromEngine[];
  beforeEach(() => {
    vi.useFakeTimers();
    workers = [];
    got = [];
    t = new WorkerTransport({ watchdogMs: 1000, createWorker: () => (workers.push(new FakeWorker()), workers.at(-1) as unknown as Worker) });
    t.onMessage((m) => got.push(m));
    workers[0].out({ kind: "ready" });
  });
  afterEach(() => {
    t.dispose();
    vi.useRealTimers();
  });

  it("neuer Worker bekommt BlockKeys vor SetSource, Liste bleibt über Neustarts", () => {
    const src: ToEngine = { type: "SetSource", text: "fillet(1) cube(5);", version: 1 };
    t.send({ type: "SetContext", overrides: {} });
    t.send(src);
    workers[0].out({ kind: "out", msgs: [progress("aa", 3)], busy: true });
    vi.advanceTimersByTime(1001);
    expect(workers[0].terminated).toBe(true);
    expect(t.restarts).toBe(1);
    const err = got.find((m) => m.type === "Error") as Extract<FromEngine, { type: "Error" }>;
    expect(err.message).toContain("Zeile 3");
    expect(err.message).toContain("Knoten gesperrt");
    expect(err.node).toBe(4);
    workers[1].out({ kind: "ready" });
    expect(workers[1].posted.map((m) => m.type)).toEqual(["BlockKeys", "SetContext", "SetSource"]);
    expect(workers[1].posted[0]).toEqual({ type: "BlockKeys", keys: ["aa"] });
    expect(workers[1].posted[2]).toEqual(src);

    // A second hang at another node: both keys go to the next worker.
    workers[1].out({ kind: "out", msgs: [progress("bb", 7)], busy: true });
    vi.advanceTimersByTime(1001);
    workers[2].out({ kind: "ready" });
    expect(workers[2].posted[0]).toEqual({ type: "BlockKeys", keys: ["aa", "bb"] });
    expect(workers[2].posted.at(-1)).toEqual(src);
    expect([...t.blockedKeys]).toEqual(["aa", "bb"]);
  });

  it("ohne neuen Knoten wird die Quelle nicht erneut gesendet", () => {
    t.send({ type: "SetSource", text: "x", version: 1 });
    // Hang without progress (no node known).
    vi.advanceTimersByTime(1001);
    workers[1].out({ kind: "ready" });
    expect(workers[1].posted).toEqual([]);
    // Same node again: already blocked, the source would hang the same way.
    t.send({ type: "SetSource", text: "x", version: 2 });
    workers[1].out({ kind: "out", msgs: [progress("aa", 1)], busy: true });
    vi.advanceTimersByTime(1001);
    workers[2].out({ kind: "ready" });
    expect(workers[2].posted.map((m) => m.type)).toEqual(["BlockKeys", "SetSource"]);
    workers[2].out({ kind: "out", msgs: [progress("aa", 1)], busy: true });
    vi.advanceTimersByTime(1001);
    workers[3].out({ kind: "ready" });
    expect(workers[3].posted.map((m) => m.type)).toEqual(["BlockKeys"]);
  });

  it("SetLang geht zuerst an jeden neuen Worker", () => {
    t.send({ type: "SetLang", lang: "de" });
    t.send({ type: "SetSource", text: "x", version: 1 });
    workers[0].out({ kind: "out", msgs: [progress("cc", 2)], busy: true });
    vi.advanceTimersByTime(1001);
    workers[1].out({ kind: "ready" });
    expect(workers[1].posted.map((m) => m.type)).toEqual(["SetLang", "BlockKeys", "SetSource"]);
    expect(workers[1].posted[0]).toEqual({ type: "SetLang", lang: "de" });
  });
});
