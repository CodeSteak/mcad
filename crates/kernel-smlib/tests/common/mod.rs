#![allow(dead_code)]

use cad_kernel::{Kernel, Mesh};
use glam::{DMat4, DVec2, DVec3};
use kernel_smlib::{Shape, SmlibKernel};
use std::f64::consts::PI;

pub const PI_: f64 = PI;

pub fn k() -> SmlibKernel {
    SmlibKernel::new()
}

pub fn approx(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(1e-9)
}

#[track_caller]
pub fn assert_approx(what: &str, got: f64, want: f64, rel: f64) {
    assert!(approx(got, want, rel), "{what}: got {got}, want {want} (rel {rel})");
}

pub fn volume(k: &mut SmlibKernel, s: &Shape) -> f64 {
    k.mass_props(s).expect("mass_props").volume
}

pub fn translate(x: f64, y: f64, z: f64) -> DMat4 {
    DMat4::from_translation(DVec3::new(x, y, z))
}

/// Signed volume of a triangle mesh (divergence theorem).
pub fn mesh_volume(m: &Mesh) -> f64 {
    let p = |i: u32| {
        let i = i as usize * 3;
        DVec3::new(m.positions[i] as f64, m.positions[i + 1] as f64, m.positions[i + 2] as f64)
    };
    m.indices.chunks_exact(3).map(|t| p(t[0]).dot(p(t[1]).cross(p(t[2]))) / 6.0).sum()
}

/// Every undirected edge of the mesh, keyed by quantised positions, must be used by exactly two
/// triangles (closed), with opposite directions (consistently oriented).
pub fn mesh_closed(m: &Mesh) -> bool {
    use std::collections::HashMap;
    let q = |i: u32| {
        let i = i as usize * 3;
        let s = 1e4;
        (
            (m.positions[i] as f64 * s).round() as i64,
            (m.positions[i + 1] as f64 * s).round() as i64,
            (m.positions[i + 2] as f64 * s).round() as i64,
        )
    };
    let mut count: HashMap<((i64, i64, i64), (i64, i64, i64)), i32> = HashMap::new();
    for t in m.indices.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (qa, qb) = (q(a), q(b));
            if qa == qb {
                continue;
            }
            *count.entry((qa, qb)).or_default() += 1;
        }
    }
    let mut ok = true;
    for (&(a, b), &c) in &count {
        let back = count.get(&(b, a)).copied().unwrap_or(0);
        if c != back {
            ok = false;
        }
    }
    ok
}

pub fn square(s: f64) -> Vec<DVec2> {
    vec![DVec2::new(0.0, 0.0), DVec2::new(s, 0.0), DVec2::new(s, s), DVec2::new(0.0, s)]
}
