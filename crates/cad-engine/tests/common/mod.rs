#![allow(dead_code)]

use std::collections::HashMap;

use cad_engine::{CountingKernel, Engine, Evaluation, Severity, Status};
use cad_ir::NodeIdx;
use cad_kernel::Mesh;
use glam::DVec3;
use kernel_smlib::{Shape, SmlibKernel};

pub type E = Engine<CountingKernel<SmlibKernel>>;
pub type Ev = Evaluation<Shape>;

pub fn engine() -> E {
    Engine::new(CountingKernel::new(SmlibKernel::new()))
}

pub fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

#[track_caller]
pub fn assert_approx(what: &str, got: f64, want: f64, rel: f64) {
    assert!((got - want).abs() <= rel * want.abs().max(1e-9), "{what}: got {got}, want {want} (rel {rel})");
}

pub fn volume(e: &mut E, ev: &Ev, n: NodeIdx) -> f64 {
    e.mass_props(ev, n).expect("mass_props").volume
}

pub fn root_volume(e: &mut E, ev: &Ev) -> f64 {
    let r = ev.root().unwrap();
    volume(e, ev, r)
}

pub fn errors(ev: &Ev) -> Vec<String> {
    ev.diagnostics().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect()
}

pub fn warnings(ev: &Ev) -> Vec<String> {
    ev.diagnostics().filter(|d| d.severity == Severity::Warning).map(|d| d.message.clone()).collect()
}

#[track_caller]
pub fn assert_clean(ev: &Ev) {
    let d: Vec<_> = ev.diagnostics().collect();
    assert!(d.is_empty(), "unexpected diagnostics: {d:#?}");
    assert!(ev.reports.iter().all(|r| matches!(r.status, Status::Ok | Status::Skipped)));
}

/// Signed volume of a triangle mesh.
pub fn mesh_volume(m: &Mesh) -> f64 {
    let p = |i: u32| {
        let i = i as usize * 3;
        DVec3::new(m.positions[i] as f64, m.positions[i + 1] as f64, m.positions[i + 2] as f64)
    };
    m.indices.chunks_exact(3).map(|t| p(t[0]).dot(p(t[1]).cross(p(t[2]))) / 6.0).sum()
}

/// Every directed edge (quantised positions) has a matching opposite edge.
pub fn tris_closed(tris: &[[[f32; 3]; 3]]) -> bool {
    let q = |p: [f32; 3]| {
        let s = 1e4;
        ((p[0] as f64 * s).round() as i64, (p[1] as f64 * s).round() as i64, (p[2] as f64 * s).round() as i64)
    };
    let mut count: HashMap<_, i32> = HashMap::new();
    for t in tris {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (qa, qb) = (q(a), q(b));
            if qa != qb {
                *count.entry((qa, qb)).or_default() += 1;
            }
        }
    }
    count.iter().all(|(&(a, b), &c)| count.get(&(b, a)).copied().unwrap_or(0) == c)
}

pub fn mesh_tris(m: &Mesh) -> Vec<[[f32; 3]; 3]> {
    let p = |i: u32| {
        let i = i as usize * 3;
        [m.positions[i], m.positions[i + 1], m.positions[i + 2]]
    };
    m.indices.chunks_exact(3).map(|t| [p(t[0]), p(t[1]), p(t[2])]).collect()
}
