// Node smoke test of the browser engine: smlib.wasm + cad-wasm (wasm-bindgen, --target web)
// in one process, driven through the protocol like the worker does.
//
//   web/scripts/build-wasm.sh && node web/scripts/smoke-node.mjs [--json]
//
// Checks: SetSource with the spec example `gehaeuse` → preliminary scene → stepwise (16 ms
// budget) until the complete scene → root mesh with triangles; RequestMeshes; dragging a
// handle gives edits; the edited source re-evaluates from the cache; STL export.
// Prints timings and module sizes.

import assert from "node:assert/strict";
import { readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { performance } from "node:perf_hooks";
import { gzipSync } from "node:zlib";

const here = dirname(fileURLToPath(import.meta.url));
const pkg = join(here, "../src/wasm-pkg");
const asJson = process.argv.includes("--json");

const GEHAEUSE = `module gehaeuse(w = 40, d = 30, h = 20, t = 3) {
    grid g {
        x: links = 0, innen_l = t, innen_r = w - t, rechts = w;
        y: vorne = 0, hinten = d;
        z: boden = 0, deckel = h;
    }

    difference() {
        box(g[links, vorne, boden], g[rechts, hinten, deckel]);
        box(g[innen_l, vorne, t], g[innen_r, hinten, deckel]);
    }

    anchor ecke = g[rechts, hinten, deckel];
    anchor oben = g[links, vorne, deckel], dir = [0, 0, 1];
}

module deckel() cube([10, 10, 2]);

k: gehaeuse(w = 60);
translate(k.ecke) deckel();

grid h {
    x: rand = k.ecke.x;
}
`;

const ms = (t) => Math.round(t * 10) / 10;
const timings = {};

// ---- load
let t0 = performance.now();
const { default: createSmlib } = await import(pathToFileURL(join(pkg, "smlib.js")).href);
const { initSmlib } = await import(pathToFileURL(join(pkg, "smlib-bridge.js")).href);
await initSmlib(createSmlib, { wasmBinary: readFileSync(join(pkg, "smlib.wasm")) });
timings.load_smlib_ms = ms(performance.now() - t0);

t0 = performance.now();
const cad = await import(pathToFileURL(join(pkg, "cad_wasm.js")).href);
await cad.default({ module_or_path: readFileSync(join(pkg, "cad_wasm_bg.wasm")) });
const session = new cad.Session();
timings.load_cad_wasm_ms = ms(performance.now() - t0);
assert.equal(cad.Session.protocolVersion(), 1);

/** Runs steps with a 16 ms budget until idle; calls `on(msg, tMs)` for every message. */
function runSteps(t0, on) {
  let steps = 0;
  while (session.busy()) {
    for (const m of session.step(16)) on(m, performance.now() - t0);
    steps++;
    assert.ok(steps < 100000, "no progress");
  }
  return steps;
}

// ---- SetSource → scene → meshes
t0 = performance.now();
const first = session.post({ type: "SetSource", text: GEHAEUSE, version: 1 });
const tPrelim = performance.now() - t0;
assert.equal(first[0].type, "Diagnostics");
assert.deepEqual(first[0].items, []);
const prelim = first.find((m) => m.type === "Scene");
assert.ok(prelim && prelim.complete === false, "preliminary scene");
assert.ok(prelim.handles.length > 0, "handles in the preliminary scene");
let scene = null;
let rootMesh = null;
let tScene = 0;
let tMesh = 0;
let progress = 0;
const steps = runSteps(t0, (m, t) => {
  if (m.type === "Progress") progress++;
  if (m.type === "Scene") {
    scene = m;
    tScene = t;
  }
  if (m.type === "Meshes" && !rootMesh) {
    rootMesh = m.items[0];
    tMesh = t;
  }
  if (m.type === "Error") throw new Error(m.message);
});
timings.set_source_to_preliminary_scene_ms = ms(tPrelim);
timings.set_source_to_scene_ms = ms(tScene);
timings.set_source_to_root_mesh_ms = ms(tMesh);
timings.steps = steps;

assert.ok(scene && scene.complete, "complete scene");
assert.equal(scene.version, 1);
assert.ok(scene.nodes.every((n) => n.status === "ok"), JSON.stringify(scene.nodes.map((n) => n.status)));
assert.equal(scene.grids.length, 2);
const ecke = scene.anchors.find((a) => a.text === "k.ecke");
assert.deepEqual(ecke.point, [60, 30, 20]);
assert.equal(scene.nodes[0].stmt, null, "optional fields are null");
assert.ok(rootMesh, "root mesh");
assert.equal(rootMesh.node, scene.root);
assert.ok(rootMesh.positions instanceof Float32Array);
assert.ok(rootMesh.indices instanceof Uint32Array);
const tris = rootMesh.indices.length / 3;
assert.ok(tris > 0, "triangles");
assert.equal(rootMesh.tri_tags.length, tris);

// ---- RequestMeshes for every node with geometry
const wanted = scene.nodes.filter((n) => n.mesh_key && n.status !== "skipped").map((n) => n.idx);
t0 = performance.now();
assert.deepEqual(session.post({ type: "RequestMeshes", nodes: wanted, version: 1 }), []);
let items = [];
runSteps(t0, (m) => {
  if (m.type === "Meshes") items = items.concat(m.items);
});
timings.request_all_meshes_ms = ms(performance.now() - t0);
const keys = new Set(wanted.map((i) => scene.nodes[i].mesh_key));
assert.equal(items.length, keys.size, "one mesh per distinct key");

// ---- drag a handle: translate(k.ecke) target, free (Alt) and snapped
const h = scene.handles.find((x) => x.kind.kind === "point" && x.range.line === 20);
assert.ok(h, "translate handle");
const start = session.post({ type: "DragStart", handle: h.id });
assert.equal(start[0].type, "SnapTargets");
assert.ok(start[0].targets.some((t) => t.text === "k.oben"), "anchor snap target");
const drag = session.post({ type: "Drag", handle: h.id, target: { kind: "point", pos: [12.4, 30, 20] } });
assert.equal(drag.length, 1);
assert.equal(drag[0].type, "Edits");
assert.ok(drag[0].edits.length > 0, "edits");
const apply = (src, edits) =>
  [...edits].sort((a, b) => b.from - a.from).reduce((s, e) => s.slice(0, e.from) + e.text + s.slice(e.to), src);
const v2 = apply(GEHAEUSE, drag[0].edits);
assert.ok(v2.includes("translate([12, 30, 20]) deckel();"), v2);
const snap = session.post({
  type: "Drag",
  handle: h.id,
  target: { kind: "snap", snap: start[0].targets.find((t) => t.text === "k.oben").snap },
});
assert.equal(apply(GEHAEUSE, snap[0].edits).includes("translate(k.oben) deckel();"), true);

// ---- edited source during the drag: cache hits, coarse mesh
t0 = performance.now();
session.post({ type: "SetSource", text: v2, version: 2 });
let scene2 = null;
let mesh2 = null;
runSteps(t0, (m, t) => {
  if (m.type === "Scene") {
    scene2 = m;
    timings.edit_to_scene_ms = ms(t);
  }
  if (m.type === "Meshes" && !mesh2) {
    mesh2 = m.items[0];
    timings.edit_to_coarse_root_mesh_ms = ms(t);
  }
});
assert.ok(scene2.stats.cache_hits > 0, JSON.stringify(scene2.stats));
assert.equal(mesh2.coarse, true);
session.post({ type: "DragEnd", handle: h.id });
let fine = null;
runSteps(performance.now(), (m) => {
  if (m.type === "Meshes") fine = m.items[0];
});
assert.equal(fine.coarse, false);

// ---- warm engine, new geometry (no cache hits for the changed nodes)
const v3 = GEHAEUSE.replace("w = 40, d = 30, h = 20, t = 3", "w = 41, d = 31, h = 21, t = 2").replace("w = 60", "w = 62");
t0 = performance.now();
const pre3 = session.post({ type: "SetSource", text: v3, version: 4 });
timings.warm_set_source_to_preliminary_scene_ms = ms(performance.now() - t0);
assert.ok(pre3.some((m) => m.type === "Scene"));
let got3 = false;
runSteps(t0, (m, t) => {
  if (m.type === "Scene") timings.warm_set_source_to_scene_ms = ms(t);
  if (m.type === "Meshes" && !got3) {
    got3 = true;
    timings.warm_set_source_to_root_mesh_ms = ms(t);
  }
});
assert.ok(got3);

// ---- syntax error keeps the scene
const bad = session.post({ type: "SetSource", text: "cube(10;", version: 3 });
assert.equal(bad.length, 1);
assert.equal(bad[0].type, "Diagnostics");
assert.equal(bad[0].items[0].source, "syntax");
assert.equal(session.busy(), false);

// ---- export
session.post({ type: "Export", format: "stl" });
let exp = null;
runSteps(performance.now(), (m) => {
  if (m.type === "ExportResult") exp = m;
});
assert.ok(exp.bytes instanceof Uint8Array);
assert.equal(exp.bytes.length, 84 + 50 * exp.triangles);

const size = (f) => {
  const b = readFileSync(join(pkg, f));
  return { raw: statSync(join(pkg, f)).size, gzip: gzipSync(b, { level: 9 }).length };
};
const report = {
  ok: true,
  triangles_root: tris,
  nodes: scene.nodes.length,
  timings,
  sizes: { cad_wasm: size("cad_wasm_bg.wasm"), smlib: size("smlib.wasm") },
};
if (asJson) console.log(JSON.stringify(report, null, 2));
else {
  console.log("smoke-node: ok");
  for (const [k, v] of Object.entries(timings)) console.log(`  ${k}: ${v}`);
  console.log(`  root triangles: ${tris}, nodes: ${scene.nodes.length}, progress messages: ${progress}`);
  for (const [k, v] of Object.entries(report.sizes)) console.log(`  ${k}: ${v.raw} B (gzip ${v.gzip} B)`);
}
