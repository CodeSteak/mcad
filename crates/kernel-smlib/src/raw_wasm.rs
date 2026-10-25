//! Raw facade calls, wasm32-unknown-unknown: smlib.wasm is a separate Emscripten module.
//! `js/smlib-bridge.js` copies arguments between the two linear memories; the page/worker
//! must call its `initSmlib(createSmlib)` before the kernel is used.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/js/smlib-bridge.js")]
extern "C" {
    #[wasm_bindgen(js_name = sc_init)]
    fn js_init();
    #[wasm_bindgen(js_name = sc_version)]
    fn js_version() -> u32;
    #[wasm_bindgen(js_name = sc_set_draft)]
    fn js_set_draft(on: i32);
    #[wasm_bindgen(js_name = sc_last_status)]
    fn js_last_status() -> i32;
    #[wasm_bindgen(js_name = sc_last_message)]
    fn js_last_message() -> String;
    #[wasm_bindgen(js_name = sc_last_edges)]
    fn js_last_edges() -> Vec<u32>;
    #[wasm_bindgen(js_name = sc_out_f64)]
    fn js_out_f64(slot: u32) -> Vec<f64>;
    #[wasm_bindgen(js_name = sc_out_f32)]
    fn js_out_f32(slot: u32) -> Vec<f32>;
    #[wasm_bindgen(js_name = sc_out_u32)]
    fn js_out_u32(slot: u32) -> Vec<u32>;
    #[wasm_bindgen(js_name = sc_retain)]
    fn js_retain(h: u32);
    #[wasm_bindgen(js_name = sc_release)]
    fn js_release(h: u32);
    #[wasm_bindgen(js_name = sc_live_handles)]
    fn js_live_handles() -> u32;
    #[wasm_bindgen(js_name = sc_make_box)]
    fn js_make_box(x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64, tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_make_sphere)]
    fn js_make_sphere(r: f64, tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_make_cylinder)]
    fn js_make_cylinder(h: f64, r1: f64, r2: f64, tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_make_torus)]
    fn js_make_torus(big_r: f64, r: f64, tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_make_polyhedron)]
    fn js_make_polyhedron(points: &[f64], sizes: &[u32], indices: &[u32], tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_transform)]
    fn js_transform(h: u32, m: &[f64]) -> u32;
    #[wasm_bindgen(js_name = sc_boolean)]
    fn js_boolean(op: i32, hs: &[u32], tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_fillet)]
    fn js_fillet(h: u32, edges: &[u32], r: f64, tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_chamfer)]
    fn js_chamfer(h: u32, edges: &[u32], d: f64, tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_law_stations)]
    fn js_law_stations(h: u32, edges: &[u32], counts: &[u32]) -> i32;
    #[wasm_bindgen(js_name = sc_fillet_var)]
    fn js_fillet_var(h: u32, edges: &[u32], counts: &[u32], radii: &[f64], tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_extrude)]
    fn js_extrude(lp: &[f64], height: f64, twist: f64, sx: f64, sy: f64, tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_revolve)]
    fn js_revolve(lp: &[f64], angle: f64, tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_loft)]
    fn js_loft(lp: &[f64], n_sections: u32, ruled: i32, tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_sweep)]
    fn js_sweep(lp: &[f64], path: &[f64], interp: i32, tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_sketch_sweep)]
    fn js_sketch_sweep(lp: &[f64], path: &[f64], mode: i32, tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_nurbs_surface)]
    fn js_nurbs_surface(pts: &[f64], rows: u32, cols: u32, tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_thicken)]
    fn js_thicken(h: u32, t: f64, tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_shell)]
    fn js_shell(h: u32, t: f64, open: &[u32], tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_offset)]
    fn js_offset(h: u32, d: f64, mode: i32, tag: u32) -> u32;
    #[wasm_bindgen(js_name = sc_edges)]
    fn js_edges(h: u32, samples: u32) -> i32;
    #[wasm_bindgen(js_name = sc_faces)]
    fn js_faces(h: u32, samples: u32) -> i32;
    #[wasm_bindgen(js_name = sc_classify)]
    fn js_classify(h: u32, pts: &[f64]) -> i32;
    #[wasm_bindgen(js_name = sc_tessellate)]
    fn js_tessellate(h: u32, tol: f64) -> i32;
    #[wasm_bindgen(js_name = sc_mass_props)]
    fn js_mass_props(h: u32) -> i32;
    #[wasm_bindgen(js_name = sc_bounds)]
    fn js_bounds(h: u32) -> i32;
    #[wasm_bindgen(js_name = sc_is_empty)]
    fn js_is_empty(h: u32) -> i32;
    #[wasm_bindgen(js_name = sc_is_solid)]
    fn js_is_solid(h: u32) -> i32;
    #[wasm_bindgen(js_name = sc_face_count)]
    fn js_face_count(h: u32) -> i32;
    #[wasm_bindgen(js_name = sc_edge_count)]
    fn js_edge_count(h: u32) -> i32;
    #[wasm_bindgen(js_name = sc_brep)]
    fn js_brep(h: u32) -> i32;
}

pub fn init() {
    js_init()
}
pub fn version() -> u32 {
    js_version()
}
pub fn set_draft(on: bool) {
    js_set_draft(on as i32)
}
pub fn last_status() -> i32 {
    js_last_status()
}
pub fn last_message() -> String {
    js_last_message()
}
pub fn last_edges() -> Vec<u32> {
    js_last_edges()
}
pub fn out_f64(slot: u32) -> Vec<f64> {
    js_out_f64(slot)
}
pub fn out_f32(slot: u32) -> Vec<f32> {
    js_out_f32(slot)
}
pub fn out_u32(slot: u32) -> Vec<u32> {
    js_out_u32(slot)
}
#[allow(dead_code)]
pub fn retain(h: u32) {
    js_retain(h)
}
pub fn release(h: u32) {
    js_release(h)
}
pub fn live_handles() -> u32 {
    js_live_handles()
}
pub fn make_box(min: [f64; 3], max: [f64; 3], tag: u32) -> u32 {
    js_make_box(min[0], min[1], min[2], max[0], max[1], max[2], tag)
}
pub fn make_sphere(r: f64, tag: u32) -> u32 {
    js_make_sphere(r, tag)
}
pub fn make_cylinder(h: f64, r1: f64, r2: f64, tag: u32) -> u32 {
    js_make_cylinder(h, r1, r2, tag)
}
pub fn make_torus(big_r: f64, r: f64, tag: u32) -> u32 {
    js_make_torus(big_r, r, tag)
}
pub fn make_polyhedron(points: &[f64], sizes: &[u32], indices: &[u32], tag: u32) -> u32 {
    js_make_polyhedron(points, sizes, indices, tag)
}
pub fn transform(h: u32, m: &[f64; 16]) -> u32 {
    js_transform(h, m)
}
pub fn boolean(op: i32, hs: &[u32], tag: u32) -> u32 {
    js_boolean(op, hs, tag)
}
pub fn fillet(h: u32, edges: &[u32], r: f64, tag: u32) -> u32 {
    js_fillet(h, edges, r, tag)
}
pub fn chamfer(h: u32, edges: &[u32], d: f64, tag: u32) -> u32 {
    js_chamfer(h, edges, d, tag)
}
pub fn law_stations(h: u32, edges: &[u32], counts: &[u32]) -> i32 {
    js_law_stations(h, edges, counts)
}
pub fn fillet_var(h: u32, edges: &[u32], counts: &[u32], radii: &[f64], tag: u32) -> u32 {
    js_fillet_var(h, edges, counts, radii, tag)
}
pub fn extrude(lp: &[f64], height: f64, twist: f64, sx: f64, sy: f64, tag: u32) -> u32 {
    js_extrude(lp, height, twist, sx, sy, tag)
}
pub fn revolve(lp: &[f64], angle: f64, tag: u32) -> u32 {
    js_revolve(lp, angle, tag)
}
pub fn loft(lp: &[f64], n_sections: u32, ruled: bool, tag: u32) -> u32 {
    js_loft(lp, n_sections, ruled as i32, tag)
}
pub fn sweep(lp: &[f64], path: &[f64], interp: i32, tag: u32) -> u32 {
    js_sweep(lp, path, interp, tag)
}
pub fn sketch_sweep(lp: &[f64], path: &[f64], mode: i32, tag: u32) -> u32 {
    js_sketch_sweep(lp, path, mode, tag)
}
pub fn nurbs_surface(pts: &[f64], rows: u32, cols: u32, tag: u32) -> u32 {
    js_nurbs_surface(pts, rows, cols, tag)
}
pub fn thicken(h: u32, t: f64, tag: u32) -> u32 {
    js_thicken(h, t, tag)
}
pub fn shell(h: u32, t: f64, open: &[u32], tag: u32) -> u32 {
    js_shell(h, t, open, tag)
}
pub fn offset(h: u32, d: f64, mode: i32, tag: u32) -> u32 {
    js_offset(h, d, mode, tag)
}
pub fn edges(h: u32, samples: u32) -> i32 {
    js_edges(h, samples)
}
pub fn faces(h: u32, samples: u32) -> i32 {
    js_faces(h, samples)
}
pub fn classify(h: u32, pts: &[f64]) -> i32 {
    js_classify(h, pts)
}
pub fn tessellate(h: u32, tol: f64) -> i32 {
    js_tessellate(h, tol)
}
pub fn mass_props(h: u32) -> i32 {
    js_mass_props(h)
}
pub fn bounds(h: u32) -> i32 {
    js_bounds(h)
}
pub fn is_empty(h: u32) -> i32 {
    js_is_empty(h)
}
pub fn is_solid(h: u32) -> i32 {
    js_is_solid(h)
}
pub fn face_count(h: u32) -> i32 {
    js_face_count(h)
}
pub fn edge_count(h: u32) -> i32 {
    js_edge_count(h)
}
pub fn brep(h: u32) -> i32 {
    js_brep(h)
}
