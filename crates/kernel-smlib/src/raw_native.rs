//! Raw facade calls, native: the C++ facade is linked statically (see build.rs).

use std::os::raw::c_char;

mod ffi {
    use std::os::raw::c_char;
    unsafe extern "C" {
        pub fn sc_init();
        pub fn sc_version() -> u32;
        pub fn sc_set_draft(on: i32);
        pub fn sc_last_status() -> i32;
        pub fn sc_last_message() -> *const c_char;
        pub fn sc_last_message_len() -> u32;
        pub fn sc_last_edges() -> *const u32;
        pub fn sc_last_edges_len() -> u32;
        pub fn sc_out_f64(slot: u32) -> *const f64;
        pub fn sc_out_f64_len(slot: u32) -> u32;
        pub fn sc_out_f32(slot: u32) -> *const f32;
        pub fn sc_out_f32_len(slot: u32) -> u32;
        pub fn sc_out_u32(slot: u32) -> *const u32;
        pub fn sc_out_u32_len(slot: u32) -> u32;
        #[allow(dead_code)]
        pub fn sc_retain(h: u32);
        pub fn sc_release(h: u32);
        pub fn sc_live_handles() -> u32;
        pub fn sc_make_box(x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64, tag: u32) -> u32;
        pub fn sc_make_sphere(r: f64, tag: u32) -> u32;
        pub fn sc_make_cylinder(h: f64, r1: f64, r2: f64, tag: u32) -> u32;
        pub fn sc_make_torus(big_r: f64, r: f64, tag: u32) -> u32;
        pub fn sc_make_polyhedron(
            points: *const f64,
            n_points: u32,
            face_sizes: *const u32,
            n_faces: u32,
            indices: *const u32,
            n_indices: u32,
            tag: u32,
        ) -> u32;
        pub fn sc_transform(h: u32, m16: *const f64) -> u32;
        pub fn sc_boolean(op: i32, hs: *const u32, n: u32, tag: u32) -> u32;
        pub fn sc_fillet(h: u32, edges: *const u32, n: u32, r: f64, tag: u32) -> u32;
        pub fn sc_chamfer(h: u32, edges: *const u32, n: u32, d: f64, tag: u32) -> u32;
        pub fn sc_law_stations(h: u32, edges: *const u32, n: u32, counts: *const u32) -> i32;
        pub fn sc_fillet_var(
            h: u32,
            edges: *const u32,
            n: u32,
            counts: *const u32,
            radii: *const f64,
            tag: u32,
        ) -> u32;
        pub fn sc_extrude(
            lp: *const f64,
            len: u32,
            height: f64,
            twist: f64,
            sx: f64,
            sy: f64,
            tag: u32,
        ) -> u32;
        pub fn sc_revolve(lp: *const f64, len: u32, angle: f64, tag: u32) -> u32;
        pub fn sc_loft(lp: *const f64, len: u32, n_sections: u32, ruled: i32, tag: u32) -> u32;
        pub fn sc_sweep(
            lp: *const f64,
            len: u32,
            path: *const f64,
            n_path: u32,
            interp: i32,
            tag: u32,
        ) -> u32;
        pub fn sc_nurbs_surface(pts: *const f64, rows: u32, cols: u32, tag: u32) -> u32;
        pub fn sc_thicken(h: u32, t: f64, tag: u32) -> u32;
        pub fn sc_shell(h: u32, t: f64, open: *const u32, n: u32, tag: u32) -> u32;
        pub fn sc_edges(h: u32, samples: u32) -> i32;
        pub fn sc_faces(h: u32, samples: u32) -> i32;
        pub fn sc_classify(h: u32, pts: *const f64, n: u32) -> i32;
        pub fn sc_tessellate(h: u32, tol: f64) -> i32;
        pub fn sc_mass_props(h: u32) -> i32;
        pub fn sc_bounds(h: u32) -> i32;
        pub fn sc_is_empty(h: u32) -> i32;
        pub fn sc_is_solid(h: u32) -> i32;
        pub fn sc_face_count(h: u32) -> i32;
        pub fn sc_edge_count(h: u32) -> i32;
        pub fn sc_brep(h: u32) -> i32;
    }
}

unsafe fn slice<T: Copy>(p: *const T, n: u32) -> Vec<T> {
    if p.is_null() || n == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(p, n as usize).to_vec() }
    }
}

fn len(n: usize) -> u32 {
    u32::try_from(n).expect("array too large for the facade")
}

pub fn init() {
    unsafe { ffi::sc_init() }
}
pub fn version() -> u32 {
    unsafe { ffi::sc_version() }
}
pub fn set_draft(on: bool) {
    unsafe { ffi::sc_set_draft(on as i32) }
}
pub fn last_status() -> i32 {
    unsafe { ffi::sc_last_status() }
}
pub fn last_message() -> String {
    unsafe {
        let p: *const c_char = ffi::sc_last_message();
        let bytes = slice(p as *const u8, ffi::sc_last_message_len());
        String::from_utf8_lossy(&bytes).into_owned()
    }
}
pub fn last_edges() -> Vec<u32> {
    unsafe { slice(ffi::sc_last_edges(), ffi::sc_last_edges_len()) }
}
pub fn out_f64(slot: u32) -> Vec<f64> {
    unsafe { slice(ffi::sc_out_f64(slot), ffi::sc_out_f64_len(slot)) }
}
pub fn out_f32(slot: u32) -> Vec<f32> {
    unsafe { slice(ffi::sc_out_f32(slot), ffi::sc_out_f32_len(slot)) }
}
pub fn out_u32(slot: u32) -> Vec<u32> {
    unsafe { slice(ffi::sc_out_u32(slot), ffi::sc_out_u32_len(slot)) }
}
#[allow(dead_code)]
pub fn retain(h: u32) {
    unsafe { ffi::sc_retain(h) }
}
pub fn release(h: u32) {
    unsafe { ffi::sc_release(h) }
}
pub fn live_handles() -> u32 {
    unsafe { ffi::sc_live_handles() }
}
pub fn make_box(min: [f64; 3], max: [f64; 3], tag: u32) -> u32 {
    unsafe { ffi::sc_make_box(min[0], min[1], min[2], max[0], max[1], max[2], tag) }
}
pub fn make_sphere(r: f64, tag: u32) -> u32 {
    unsafe { ffi::sc_make_sphere(r, tag) }
}
pub fn make_cylinder(h: f64, r1: f64, r2: f64, tag: u32) -> u32 {
    unsafe { ffi::sc_make_cylinder(h, r1, r2, tag) }
}
pub fn make_torus(big_r: f64, r: f64, tag: u32) -> u32 {
    unsafe { ffi::sc_make_torus(big_r, r, tag) }
}
pub fn make_polyhedron(points: &[f64], sizes: &[u32], indices: &[u32], tag: u32) -> u32 {
    unsafe {
        ffi::sc_make_polyhedron(
            points.as_ptr(),
            len(points.len() / 3),
            sizes.as_ptr(),
            len(sizes.len()),
            indices.as_ptr(),
            len(indices.len()),
            tag,
        )
    }
}
pub fn transform(h: u32, m: &[f64; 16]) -> u32 {
    unsafe { ffi::sc_transform(h, m.as_ptr()) }
}
pub fn boolean(op: i32, hs: &[u32], tag: u32) -> u32 {
    unsafe { ffi::sc_boolean(op, hs.as_ptr(), len(hs.len()), tag) }
}
pub fn fillet(h: u32, edges: &[u32], r: f64, tag: u32) -> u32 {
    unsafe { ffi::sc_fillet(h, edges.as_ptr(), len(edges.len()), r, tag) }
}
pub fn chamfer(h: u32, edges: &[u32], d: f64, tag: u32) -> u32 {
    unsafe { ffi::sc_chamfer(h, edges.as_ptr(), len(edges.len()), d, tag) }
}
pub fn law_stations(h: u32, edges: &[u32], counts: &[u32]) -> i32 {
    assert_eq!(edges.len(), counts.len());
    unsafe { ffi::sc_law_stations(h, edges.as_ptr(), len(edges.len()), counts.as_ptr()) }
}
pub fn fillet_var(h: u32, edges: &[u32], counts: &[u32], radii: &[f64], tag: u32) -> u32 {
    assert_eq!(edges.len(), counts.len());
    assert_eq!(counts.iter().map(|&c| c as usize).sum::<usize>(), radii.len());
    unsafe {
        ffi::sc_fillet_var(h, edges.as_ptr(), len(edges.len()), counts.as_ptr(), radii.as_ptr(), tag)
    }
}
pub fn extrude(lp: &[f64], height: f64, twist: f64, sx: f64, sy: f64, tag: u32) -> u32 {
    unsafe { ffi::sc_extrude(lp.as_ptr(), len(lp.len()), height, twist, sx, sy, tag) }
}
pub fn revolve(lp: &[f64], angle: f64, tag: u32) -> u32 {
    unsafe { ffi::sc_revolve(lp.as_ptr(), len(lp.len()), angle, tag) }
}
pub fn loft(lp: &[f64], n_sections: u32, ruled: bool, tag: u32) -> u32 {
    unsafe { ffi::sc_loft(lp.as_ptr(), len(lp.len()), n_sections, ruled as i32, tag) }
}
pub fn sweep(lp: &[f64], path: &[f64], interp: i32, tag: u32) -> u32 {
    unsafe {
        ffi::sc_sweep(lp.as_ptr(), len(lp.len()), path.as_ptr(), len(path.len() / 3), interp, tag)
    }
}
pub fn nurbs_surface(pts: &[f64], rows: u32, cols: u32, tag: u32) -> u32 {
    unsafe { ffi::sc_nurbs_surface(pts.as_ptr(), rows, cols, tag) }
}
pub fn thicken(h: u32, t: f64, tag: u32) -> u32 {
    unsafe { ffi::sc_thicken(h, t, tag) }
}
pub fn shell(h: u32, t: f64, open: &[u32], tag: u32) -> u32 {
    unsafe { ffi::sc_shell(h, t, open.as_ptr(), len(open.len()), tag) }
}
pub fn edges(h: u32, samples: u32) -> i32 {
    unsafe { ffi::sc_edges(h, samples) }
}
pub fn faces(h: u32, samples: u32) -> i32 {
    unsafe { ffi::sc_faces(h, samples) }
}
pub fn classify(h: u32, pts: &[f64]) -> i32 {
    unsafe { ffi::sc_classify(h, pts.as_ptr(), len(pts.len() / 3)) }
}
pub fn tessellate(h: u32, tol: f64) -> i32 {
    unsafe { ffi::sc_tessellate(h, tol) }
}
pub fn mass_props(h: u32) -> i32 {
    unsafe { ffi::sc_mass_props(h) }
}
pub fn bounds(h: u32) -> i32 {
    unsafe { ffi::sc_bounds(h) }
}
pub fn is_empty(h: u32) -> i32 {
    unsafe { ffi::sc_is_empty(h) }
}
pub fn is_solid(h: u32) -> i32 {
    unsafe { ffi::sc_is_solid(h) }
}
pub fn face_count(h: u32) -> i32 {
    unsafe { ffi::sc_face_count(h) }
}
pub fn edge_count(h: u32) -> i32 {
    unsafe { ffi::sc_edge_count(h) }
}
pub fn brep(h: u32) -> i32 {
    unsafe { ffi::sc_brep(h) }
}
