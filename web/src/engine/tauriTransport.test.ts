import { describe, expect, it } from "vitest";
import type { FromEngine } from "./protocol";
import { TauriTransport, decodeFrame } from "./tauriTransport";

/** Encodes a frame like crates/cad-desktop/src/wire.rs (header padded to 4 bytes). */
function frame(method: string, body: unknown, data: Uint8Array = new Uint8Array()): ArrayBuffer {
  let h = JSON.stringify({ v: 1, id: 0, kind: "event", method, body });
  while ((8 + new TextEncoder().encode(h).length) % 4) h += " ";
  const hb = new TextEncoder().encode(h);
  const buf = new ArrayBuffer(8 + hb.length + data.length);
  const dv = new DataView(buf);
  dv.setUint32(0, hb.length, true);
  dv.setUint32(4, data.length, true);
  new Uint8Array(buf, 8, hb.length).set(hb);
  new Uint8Array(buf, 8 + hb.length).set(data);
  return buf;
}

function fakeApi() {
  const calls: Array<{ cmd: string; args?: Record<string, unknown> }> = [];
  let onmsg: ((d: ArrayBuffer) => void) | null = null;
  return {
    calls,
    push: (d: ArrayBuffer) => onmsg?.(d),
    api: {
      invoke: async (cmd: string, args?: Record<string, unknown>) => {
        calls.push({ cmd, args });
        return null;
      },
      channel: (f: (d: ArrayBuffer) => void) => {
        onmsg = f;
        return { id: 1 };
      },
    },
  };
}

describe("TauriTransport", () => {
  it("decodes binary mesh blobs as typed array views (no copy)", () => {
    const data = new Uint8Array(20);
    new Float32Array(data.buffer, 0, 3).set([1.5, -2, 3]);
    new Uint32Array(data.buffer, 12, 2).set([7, 9]);
    const f = frame("out", { busy: false, seen: 1, msgs: [{ type: "Meshes", version: 3, items: [{ key: "k", positions: { $b: [0, 3, "f32"] }, indices: { $b: [12, 2, "u32"] } }] }] }, data);
    const d = decodeFrame(f);
    const m = (d.body as { msgs: Array<{ items: Array<{ positions: Float32Array; indices: Uint32Array }> }> }).msgs[0].items[0];
    expect(m.positions).toBeInstanceOf(Float32Array);
    expect([...m.positions]).toEqual([1.5, -2, 3]);
    expect([...m.indices]).toEqual([7, 9]);
    expect(m.positions.buffer).toBe(f);
  });

  it("batches messages per microtask in order and emits engine answers", async () => {
    const f = fakeApi();
    const t = new TauriTransport(f.api);
    const got: FromEngine[] = [];
    t.onMessage((m) => got.push(m));
    t.send({ type: "SetFiles", files: {}, main: "a.scad" });
    t.send({ type: "SetSource", text: "cube(1);", version: 1 });
    t.debugHang(5);
    await new Promise((r) => setTimeout(r, 0));
    expect(f.calls[0].cmd).toBe("engine_start");
    const posts = f.calls.filter((c) => c.cmd === "engine_post");
    expect(posts.length).toBe(1);
    expect((posts[0].args!.msgs as Array<{ type: string }>).map((m) => m.type)).toEqual(["SetFiles", "SetSource", "__debugHang"]);
    f.push(frame("out", { busy: true, seen: 2, msgs: [{ type: "Progress", version: 1, done: 0, total: 1, current: null }] }));
    f.push(frame("host", { ready: true, busy: false, restarts: 2, blocked_keys: ["ab"] }));
    expect(got.map((m) => m.type)).toEqual(["Progress"]);
    expect(t.restarts).toBe(2);
    expect([...t.blockedKeys]).toEqual(["ab"]);
    expect(t.busy).toBe(true);
  });
});
