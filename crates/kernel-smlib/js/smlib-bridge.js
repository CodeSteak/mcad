// Bridge between kernel-smlib (Rust, wasm32-unknown-unknown, wasm-bindgen) and smlib.wasm
// (C++ SMLib + facade, Emscripten). Both modules have their own linear memory; arguments
// and results are copied here. Runs in the same Web Worker (or Node) as the Rust module.
//
// Usage (before the first SmlibKernel::new()):
//
//   import createSmlib from "/smlib/smlib.js";
//   import { initSmlib } from "<wasm-bindgen output>/snippets/kernel-smlib-*/js/smlib-bridge.js";
//   await initSmlib(createSmlib);              // or initSmlib(alreadyCreatedModule)
//
// All exported sc_* functions mirror smlib/facade/sc_facade.h. Functions that take arrays
// accept typed arrays (wasm-bindgen passes Float64Array / Uint32Array views of Rust memory);
// output buffers are returned as fresh typed arrays (copies).

let M = null;

/**
 * Loads the Emscripten module. `factory` is the default export of smlib.js
 * (`createSmlib`), or an already instantiated module. `options` is passed to the factory
 * (e.g. `{ locateFile: (p) => "/smlib/" + p }` or `{ wasmBinary }`).
 */
export async function initSmlib(factory, options = {}) {
  M = typeof factory === "function" ? await factory(options) : factory;
  M._sc_init();
  return M;
}

/** The loaded Emscripten module (for tests/diagnostics), or null. */
export function smlibModule() {
  return M;
}

export function smlibReady() {
  return M !== null;
}

function mod() {
  if (M === null) {
    throw new Error("smlib.wasm is not loaded: call initSmlib(createSmlib) before using the kernel");
  }
  return M;
}

// --- copying helpers -----------------------------------------------------------------

function allocF64(a) {
  const m = mod();
  const n = a ? a.length : 0;
  if (n === 0) return 0;
  const p = m._malloc(n * 8);
  m.HEAPF64.set(a, p >>> 3);
  return p;
}

function allocU32(a) {
  const m = mod();
  const n = a ? a.length : 0;
  if (n === 0) return 0;
  const p = m._malloc(n * 4);
  m.HEAPU32.set(a, p >>> 2);
  return p;
}

function free(...ptrs) {
  const m = mod();
  for (const p of ptrs) if (p) m._free(p);
}

function withF64(a, f) {
  const p = allocF64(a);
  try {
    return f(p);
  } finally {
    free(p);
  }
}

function withU32(a, f) {
  const p = allocU32(a);
  try {
    return f(p);
  } finally {
    free(p);
  }
}

// --- errors and outputs --------------------------------------------------------------

export function sc_init() {
  mod()._sc_init();
}
export function sc_version() {
  return mod()._sc_version() >>> 0;
}
export function sc_set_draft(on) {
  mod()._sc_set_draft(on | 0);
}
export function sc_last_status() {
  return mod()._sc_last_status() | 0;
}
export function sc_last_message() {
  const m = mod();
  const p = m._sc_last_message();
  const n = m._sc_last_message_len() >>> 0;
  return n ? new TextDecoder().decode(m.HEAPU8.slice(p, p + n)) : "";
}
export function sc_last_edges() {
  const m = mod();
  const p = m._sc_last_edges() >>> 0;
  const n = m._sc_last_edges_len() >>> 0;
  return n ? m.HEAPU32.slice(p >>> 2, (p >>> 2) + n) : new Uint32Array(0);
}
export function sc_out_f64(slot) {
  const m = mod();
  const p = m._sc_out_f64(slot) >>> 0;
  const n = m._sc_out_f64_len(slot) >>> 0;
  return n ? m.HEAPF64.slice(p >>> 3, (p >>> 3) + n) : new Float64Array(0);
}
export function sc_out_f32(slot) {
  const m = mod();
  const p = m._sc_out_f32(slot) >>> 0;
  const n = m._sc_out_f32_len(slot) >>> 0;
  return n ? m.HEAPF32.slice(p >>> 2, (p >>> 2) + n) : new Float32Array(0);
}
export function sc_out_u32(slot) {
  const m = mod();
  const p = m._sc_out_u32(slot) >>> 0;
  const n = m._sc_out_u32_len(slot) >>> 0;
  return n ? m.HEAPU32.slice(p >>> 2, (p >>> 2) + n) : new Uint32Array(0);
}

// --- handles ---------------------------------------------------------------------------

export function sc_retain(h) {
  mod()._sc_retain(h);
}
export function sc_release(h) {
  if (M !== null) M._sc_release(h);
}
export function sc_live_handles() {
  return mod()._sc_live_handles() >>> 0;
}

// --- primitives ------------------------------------------------------------------------

export function sc_make_box(x0, y0, z0, x1, y1, z1, tag) {
  return mod()._sc_make_box(x0, y0, z0, x1, y1, z1, tag) >>> 0;
}
export function sc_make_sphere(r, tag) {
  return mod()._sc_make_sphere(r, tag) >>> 0;
}
export function sc_make_cylinder(h, r1, r2, tag) {
  return mod()._sc_make_cylinder(h, r1, r2, tag) >>> 0;
}
export function sc_make_torus(bigR, r, tag) {
  return mod()._sc_make_torus(bigR, r, tag) >>> 0;
}
export function sc_make_polyhedron(points, sizes, indices, tag) {
  const pp = allocF64(points);
  const ps = allocU32(sizes);
  const pi = allocU32(indices);
  try {
    return (
      mod()._sc_make_polyhedron(pp, points.length / 3, ps, sizes.length, pi, indices.length, tag) >>> 0
    );
  } finally {
    free(pp, ps, pi);
  }
}

// --- operations ------------------------------------------------------------------------

export function sc_transform(h, m16) {
  return withF64(m16, (p) => mod()._sc_transform(h, p) >>> 0);
}
export function sc_boolean(op, hs, tag) {
  return withU32(hs, (p) => mod()._sc_boolean(op, p, hs.length, tag) >>> 0);
}
export function sc_fillet(h, edges, r, tag) {
  return withU32(edges, (p) => mod()._sc_fillet(h, p, edges.length, r, tag) >>> 0);
}
export function sc_chamfer(h, edges, d, tag) {
  return withU32(edges, (p) => mod()._sc_chamfer(h, p, edges.length, d, tag) >>> 0);
}
export function sc_extrude(loop, height, twist, sx, sy, tag) {
  return withF64(loop, (p) => mod()._sc_extrude(p, loop.length, height, twist, sx, sy, tag) >>> 0);
}
export function sc_revolve(loop, angle, tag) {
  return withF64(loop, (p) => mod()._sc_revolve(p, loop.length, angle, tag) >>> 0);
}
export function sc_loft(loops, nSections, ruled, tag) {
  return withF64(loops, (p) => mod()._sc_loft(p, loops.length, nSections, ruled, tag) >>> 0);
}
export function sc_sweep(loop, path, interp, tag) {
  const pl = allocF64(loop);
  const pp = allocF64(path);
  try {
    return mod()._sc_sweep(pl, loop.length, pp, path.length / 3, interp, tag) >>> 0;
  } finally {
    free(pl, pp);
  }
}
export function sc_nurbs_surface(pts, rows, cols, tag) {
  return withF64(pts, (p) => mod()._sc_nurbs_surface(p, rows, cols, tag) >>> 0);
}
export function sc_thicken(h, t, tag) {
  return mod()._sc_thicken(h, t, tag) >>> 0;
}
export function sc_shell(h, t, open, tag) {
  return withU32(open, (p) => mod()._sc_shell(h, t, p, open.length, tag) >>> 0);
}

// --- queries ---------------------------------------------------------------------------

export function sc_edges(h, samples) {
  return mod()._sc_edges(h, samples) | 0;
}
export function sc_faces(h, samples) {
  return mod()._sc_faces(h, samples) | 0;
}
export function sc_classify(h, pts) {
  return withF64(pts, (p) => mod()._sc_classify(h, p, pts.length / 3) | 0);
}
export function sc_tessellate(h, tol) {
  return mod()._sc_tessellate(h, tol) | 0;
}
export function sc_mass_props(h) {
  return mod()._sc_mass_props(h) | 0;
}
export function sc_bounds(h) {
  return mod()._sc_bounds(h) | 0;
}
export function sc_is_empty(h) {
  return mod()._sc_is_empty(h) | 0;
}
export function sc_is_solid(h) {
  return mod()._sc_is_solid(h) | 0;
}
export function sc_face_count(h) {
  return mod()._sc_face_count(h) | 0;
}
export function sc_edge_count(h) {
  return mod()._sc_edge_count(h) | 0;
}
