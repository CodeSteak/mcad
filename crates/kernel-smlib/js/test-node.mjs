// Node test of smlib.wasm through the facade, via js/smlib-bridge.js.
//
//   node crates/kernel-smlib/js/test-node.mjs [SMLIB_DIR]      (default: web/public/smlib)
//
// Also prints rough load and operation timings.

import assert from "node:assert/strict";
import { readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { performance } from "node:perf_hooks";
import * as sc from "./smlib-bridge.js";

const here = dirname(fileURLToPath(import.meta.url));
const dir = resolve(process.argv[2] ?? join(here, "../../../web/public/smlib"));

const IDENT = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
const translate = (x, y, z) => [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, x, y, z, 1];
const near = (a, b, rel) => Math.abs(a - b) <= rel * Math.max(Math.abs(b), 1e-9);

function check(h, what) {
  if (!h) throw new Error(`${what}: ${sc.sc_last_message()} (status ${sc.sc_last_status()})`);
  return h;
}
function massProps(h) {
  assert.equal(sc.sc_mass_props(h), 0, sc.sc_last_message());
  const [volume, area, cx, cy, cz] = sc.sc_out_f64(0);
  return { volume, area, centroid: [cx, cy, cz] };
}
function faces(h, samples = 2) {
  assert.equal(sc.sc_faces(h, samples), 0, sc.sc_last_message());
  const pts = sc.sc_out_f64(0);
  const meta = sc.sc_out_u32(0);
  const out = [];
  let pi = 0;
  for (let i = 0; i < meta.length; i += 2) {
    const n = meta[i + 1];
    const points = [];
    for (let k = 0; k < n; k++) points.push([pts[3 * (pi + k)], pts[3 * (pi + k) + 1], pts[3 * (pi + k) + 2]]);
    pi += n;
    out.push({ tag: meta[i], points });
  }
  return out;
}
function tessellate(h, tol) {
  assert.equal(sc.sc_tessellate(h, tol), 0, sc.sc_last_message());
  return {
    positions: sc.sc_out_f32(0),
    normals: sc.sc_out_f32(1),
    segments: sc.sc_out_f32(2),
    indices: sc.sc_out_u32(0),
    triTags: sc.sc_out_u32(1),
    triFaces: sc.sc_out_u32(2),
    edgeIds: sc.sc_out_u32(3),
  };
}
function meshVolume(m) {
  const p = (i) => [m.positions[3 * i], m.positions[3 * i + 1], m.positions[3 * i + 2]];
  let v = 0;
  for (let t = 0; t < m.indices.length; t += 3) {
    const [a, b, c] = [p(m.indices[t]), p(m.indices[t + 1]), p(m.indices[t + 2])];
    v += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6;
  }
  return v;
}

const results = [];
function test(name, f) {
  const t0 = performance.now();
  try {
    f();
    results.push([name, "ok", performance.now() - t0]);
  } catch (e) {
    results.push([name, "FAILED", performance.now() - t0]);
    console.error(`${name}: ${e.stack ?? e}`);
  }
}

// ---------------------------------------------------------------- load

const wasmPath = join(dir, "smlib.wasm");
const t0 = performance.now();
const { default: createSmlib } = await import(pathToFileURL(join(dir, "smlib.js")).href);
const wasmBinary = readFileSync(wasmPath);
const tRead = performance.now();
await sc.initSmlib(createSmlib, { wasmBinary });
const tLoad = performance.now();
console.log(`smlib.wasm ${statSync(wasmPath).size} B; read ${(tRead - t0).toFixed(1)} ms, compile+instantiate+init ${(tLoad - tRead).toFixed(1)} ms`);
assert.equal(sc.sc_version(), 3);

// ---------------------------------------------------------------- tests

test("box volume", () => {
  const b = check(sc.sc_make_box(0, 0, 0, 10, 20, 30, 1), "box");
  const mp = massProps(b);
  assert.ok(near(mp.volume, 6000, 1e-9), `${mp.volume}`);
  assert.ok(near(mp.area, 2200, 1e-6));
  assert.equal(sc.sc_is_solid(b), 1);
  assert.equal(sc.sc_face_count(b), 6);
  assert.equal(sc.sc_edge_count(b), 12);
  sc.sc_release(b);
});

let boxMinusCyl = 0;
test("box minus cylinder + provenance", () => {
  const b = check(sc.sc_make_box(0, 0, 0, 10, 10, 10, 1), "box");
  const c0 = check(sc.sc_make_cylinder(12, 2, 2, 2), "cyl");
  const c = check(sc.sc_transform(c0, new Float64Array(translate(5, 5, -1))), "transform");
  const d = check(sc.sc_boolean(1, new Uint32Array([b, c]), 50), "difference");
  sc.sc_release(b);
  sc.sc_release(c0);
  sc.sc_release(c);
  const v = massProps(d).volume;
  assert.ok(near(v, 1000 - Math.PI * 40, 1e-6), `${v}`);
  const fs = faces(d, 3);
  for (const f of fs) {
    const bore = f.points.every(([x, y]) => Math.abs(Math.hypot(x - 5, y - 5) - 2) < 1e-6);
    assert.equal(f.tag, bore ? 2 : 1);
  }
  assert.equal(fs.filter((f) => f.tag === 2).length, 1);
  boxMinusCyl = d;
});

test("fillet all edges of a box", () => {
  const b = check(sc.sc_make_box(0, 0, 0, 10, 10, 10, 1), "box");
  const f = check(sc.sc_fillet(b, new Uint32Array([...Array(12).keys()]), 1.0, 77), "fillet");
  const L = 8;
  const want = L ** 3 + 6 * L * L + 3 * Math.PI * L + (4 / 3) * Math.PI;
  assert.ok(near(massProps(f).volume, want, 1e-4));
  const fs = faces(f, 1);
  assert.equal(fs.filter((x) => x.tag === 1).length, 6);
  assert.ok(fs.filter((x) => x.tag === 77).length > 6);
  sc.sc_release(b);
  sc.sc_release(f);
});

test("fillet too large → error with edges", () => {
  const b = check(sc.sc_make_box(0, 0, 0, 10, 10, 10, 1), "box");
  const f = sc.sc_fillet(b, new Uint32Array([...Array(12).keys()]), 6.0, 77);
  assert.equal(f, 0);
  assert.equal(sc.sc_last_status(), 1);
  assert.ok(sc.sc_last_message().length > 0);
  assert.ok(sc.sc_last_edges().length > 0);
  // the module stays usable after the failure
  assert.equal(sc.sc_face_count(b), 6);
  sc.sc_release(b);
});

test("classify", () => {
  assert.equal(sc.sc_classify(boxMinusCyl, new Float64Array([1, 1, 1, 5, 5, 5, 20, 0, 0, 10, 5, 5])), 0);
  assert.deepEqual([...sc.sc_out_u32(0)], [0, 1, 1, 2]);
});

test("tessellate", () => {
  const m = tessellate(boxMinusCyl, 0.01);
  assert.ok(m.indices.length / 3 > 20);
  assert.equal(m.triTags.length, m.indices.length / 3);
  assert.ok(near(meshVolume(m), massProps(boxMinusCyl).volume, 2e-3));
  assert.deepEqual(new Set(m.triTags), new Set([1, 2]));
  assert.equal(new Set(m.edgeIds).size, sc.sc_edge_count(boxMinusCyl));
});

test("extrude, revolve, mirror", () => {
  const loop = new Float64Array([0, 4, 0, 0, 4, 0, 4, 4, 0, 4, ...IDENT]);
  const e = check(sc.sc_extrude(loop, 5, 0, 1, 1, 7), "extrude");
  assert.ok(near(massProps(e).volume, 80, 1e-9));
  const toXZ = [1, 0, 0, 0, 0, 0, 1, 0, 0, -1, 0, 0, 0, 0, 0, 1];
  const ring = new Float64Array([0, 4, 2, 0, 3, 0, 3, 1, 2, 1, ...toXZ]);
  const r = check(sc.sc_revolve(ring, 2 * Math.PI, 8), "revolve");
  assert.ok(near(massProps(r).volume, Math.PI * 5, 1e-5));
  const mir = check(sc.sc_transform(boxMinusCyl, new Float64Array([-1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1])), "mirror");
  assert.ok(near(massProps(mir).volume, massProps(boxMinusCyl).volume, 1e-6));
  [e, r, mir].forEach(sc.sc_release);
});

test("union of three orthogonal cylinders (checked boolean)", () => {
  // SMLib alone drops the -x arm in (Z ∪ Y) ∪ X; the facade checks and reorders
  const c0 = check(sc.sc_make_cylinder(30, 4, 4, 1), "cyl");
  const z = check(sc.sc_transform(c0, new Float64Array(translate(0, 0, -15))), "z");
  const y = check(sc.sc_transform(z, new Float64Array([1, 0, 0, 0, 0, 0, 1, 0, 0, -1, 0, 0, 0, 0, 0, 1])), "y");
  const x = check(sc.sc_transform(z, new Float64Array([0, 0, -1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1])), "x");
  const u = check(sc.sc_boolean(0, new Uint32Array([z, y, x]), 9), "union");
  const want = 3 * Math.PI * 16 * 30 - 16 * 64 + 8 * (2 - Math.SQRT2) * 64;
  assert.ok(near(massProps(u).volume, want, 1e-5), `${massProps(u).volume} vs ${want}`);
  assert.equal(sc.sc_bounds(u), 0);
  const bb = sc.sc_out_f64(0);
  assert.ok(bb.every((v, i) => Math.abs(v - (i < 3 ? -15 : 15)) < 1e-3), `${[...bb]}`);
  [c0, z, y, x, u].forEach(sc.sc_release);
});

test("twisted extrude: rotate, then scale (OpenSCAD)", () => {
  const loop = new Float64Array([0, 4, -5, -3, 5, -3, 5, 3, -5, 3, ...IDENT]);
  const e = check(sc.sc_extrude(loop, 12, Math.PI / 3, 1, 0.5, 7), "extrude");
  assert.ok(near(massProps(e).volume, 540, 1e-4), `${massProps(e).volume}`);
  assert.equal(sc.sc_bounds(e), 0);
  const bb = sc.sc_out_f64(0);
  assert.ok(Math.abs(bb[4] - 3.8309045) < 1e-3 && Math.abs(bb[3] - Math.sqrt(34)) < 1e-3, `${[...bb]}`);
  sc.sc_release(e);
});

test("B-rep export stream (sc_brep): box minus cylinder", () => {
  // format 1, see smlib/facade/sc_facade.h; only the head is decoded here, Rust parses the rest
  assert.equal(sc.sc_brep(boxMinusCyl), 0, sc.sc_last_message());
  const d = sc.sc_out_f64(0);
  assert.equal(d[0], 1, "format");
  const nv = d[2];
  let i = 3 + 3 * nv;
  const ne = d[i];
  assert.ok(nv >= 8 && ne >= 12, `${nv} vertices, ${ne} edges`);
  i += 1;
  // first edge: start, end, class, t0, t1, approximated, degree, n_ctrl, n_knots
  const [start, end, , t0, t1, approx, deg, nc, nk] = d.slice(i, i + 9);
  assert.ok(start < nv && end < nv && t1 > t0 && approx === 0, `${[start, end, t0, t1, approx]}`);
  assert.equal(nk, nc + deg + 1);
  assert.equal(sc.sc_brep(123456), 3);
});

test("invalid handle", () => {
  assert.equal(sc.sc_fillet(123456, new Uint32Array([0]), 1, 1), 0);
  assert.equal(sc.sc_last_status(), 3);
});

// ---------------------------------------------------------------- timing scenario

{
  const t = {};
  let s = performance.now();
  const b = check(sc.sc_make_box(0, 0, 0, 10, 10, 10, 1), "box");
  const c0 = check(sc.sc_make_cylinder(12, 2, 2, 2), "cyl");
  const c = check(sc.sc_transform(c0, new Float64Array(translate(5, 5, -1))), "transform");
  t.primitives = performance.now() - s;
  s = performance.now();
  const d = check(sc.sc_boolean(1, new Uint32Array([b, c]), 50), "difference");
  t.boolean = performance.now() - s;
  s = performance.now();
  const ids = [];
  assert.equal(sc.sc_edges(d, 2), 0);
  {
    const meta = sc.sc_out_u32(0);
    const pts = sc.sc_out_f64(0);
    let pi = 0, id = 0;
    for (let i = 0; i < meta.length; id++) {
      const np = meta[i], nt = meta[i + 1];
      let top = true;
      for (let k = 0; k < np; k++) if (Math.abs(pts[3 * (pi + k) + 2] - 10) > 1e-9) top = false;
      if (top) ids.push(id);
      pi += np;
      i += 2 + nt;
    }
  }
  t.edges = performance.now() - s;
  s = performance.now();
  const f = check(sc.sc_fillet(d, new Uint32Array(ids), 0.8, 77), "fillet");
  t.fillet = performance.now() - s;
  s = performance.now();
  const m = tessellate(f, 0.01);
  t.tessellate = performance.now() - s;
  console.log(
    `scenario box-cylinder + fillet(${ids.length} top edges) + tessellate (${m.indices.length / 3} triangles): ` +
      Object.entries(t).map(([k, v]) => `${k} ${v.toFixed(1)} ms`).join(", ") +
      `; total incl. load ${(performance.now() - t0).toFixed(0)} ms`,
  );
  [b, c0, c, d, f].forEach(sc.sc_release);
}

sc.sc_release(boxMinusCyl);
console.log(`live handles at exit: ${sc.sc_live_handles()}`);
for (const [n, r, ms] of results) console.log(`${r.padEnd(6)} ${n} (${ms.toFixed(1)} ms)`);
const failed = results.filter((r) => r[1] !== "ok").length;
if (failed) {
  console.error(`${failed} test(s) failed`);
  process.exit(1);
}
console.log("all node tests passed");
