//! Fillets and chamfers over solidcore (plan 1.3): face and edge counts as SMLib's (checked with
//! `mcad diff-kernels`), volumes against the exact value, tags of the new faces.

#![cfg(feature = "solidcore")]

use cad_kernel::{BoolOp, EdgeInfo, Kernel, KernelError, Tag};
use glam::DVec3;
use kernel_solidcore::{Shape, SolidcoreKernel};
use std::f64::consts::PI;

/// The box 10 × 20 × 30 from the origin, tag 1.
fn block(k: &mut SolidcoreKernel) -> Shape {
    k.make_box(DVec3::ZERO, DVec3::new(10.0, 20.0, 30.0), 1).unwrap()
}

/// Id of the edge of `s` from `a` to `b` (either direction).
fn edge(k: &mut SolidcoreKernel, s: &Shape, a: DVec3, b: DVec3) -> u32 {
    let es = k.edges(s, 0).unwrap();
    let same = |e: &EdgeInfo| {
        let (p, q) = (e.points[0], *e.points.last().unwrap());
        ((p - a).length() < 1e-9 && (q - b).length() < 1e-9) || ((p - b).length() < 1e-9 && (q - a).length() < 1e-9)
    };
    es.iter().find(|e| same(e)).unwrap_or_else(|| panic!("no edge {a} – {b}")).id
}

/// Faces, edges and volume of `s` (within `rel` of `exact`); the face tags, sorted.
fn check(k: &mut SolidcoreKernel, s: &Shape, faces: usize, edges: usize, exact: f64, rel: f64) -> Vec<Tag> {
    let m = k.mass_props(s).unwrap();
    let f = k.faces(s, 0).unwrap();
    let e = k.edges(s, 0).unwrap().len();
    assert_eq!((f.len(), e), (faces, edges), "faces/edges, volume {}", m.volume);
    assert!((m.volume - exact).abs() <= rel * exact, "volume {} instead of {exact}", m.volume);
    assert!(k.is_solid(s));
    let mut t: Vec<Tag> = f.iter().map(|f| f.tag).collect();
    t.sort_unstable();
    t
}

#[test]
fn fillet_and_chamfer_of_a_box_edge() {
    let mut k = SolidcoreKernel::new();
    let b = block(&mut k);
    let e = edge(&mut k, &b, DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 30.0));
    let f = k.fillet(&b, &[e], 2.0, 9).unwrap();
    let tags = check(&mut k, &f, 7, 15, 6000.0 - (1.0 - PI / 4.0) * 4.0 * 30.0, 1e-6);
    assert_eq!(tags, vec![1, 1, 1, 1, 1, 1, 9]);
    let c = k.chamfer(&b, &[e], 1.5, 9).unwrap();
    let tags = check(&mut k, &c, 7, 15, 6000.0 - 1.5 * 1.5 / 2.0 * 30.0, 1e-9);
    assert_eq!(tags, vec![1, 1, 1, 1, 1, 1, 9]);
}

#[test]
fn fillet_all_edges_of_a_box() {
    let mut k = SolidcoreKernel::new();
    let b = block(&mut k);
    let all: Vec<u32> = (0..12).collect();
    let r = 2.0;
    let f = k.fillet(&b, &all, r, 9).unwrap();
    // edges: (1 − π/4) r² per length, less what the 8 corners overlap; corners: the box corner
    // minus an eighth of a ball
    let (a, bb, c) = (10.0 - 2.0 * r, 20.0 - 2.0 * r, 30.0 - 2.0 * r);
    let exact = 6000.0 - (1.0 - PI / 4.0) * r * r * 4.0 * (a + bb + c) - 8.0 * (r * r * r - PI / 6.0 * r * r * r);
    let tags = check(&mut k, &f, 26, 48, exact, 1e-5);
    assert_eq!(tags.iter().filter(|&&t| t == 1).count(), 6);
}

#[test]
fn variable_fillet_linear_law() {
    let mut k = SolidcoreKernel::new();
    let b = block(&mut k);
    let e = edge(&mut k, &b, DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 30.0));
    let st = k.law_stations(&b, &[e], &[4]).unwrap();
    assert_eq!(st[0].len(), 4);
    // radius 1 at z = 0 to 2 at z = 30, whichever way the edge runs
    let radii: Vec<f64> = st[0].iter().map(|p| 1.0 + p.z / 30.0).collect();
    let f = k.fillet_variable(&b, &[e], &[radii], 9).unwrap();
    // ∫ (1 − π/4) r(z)² dz = (1 − π/4) · 30 · 7/3
    check(&mut k, &f, 7, 15, 6000.0 - (1.0 - PI / 4.0) * 70.0, 1e-5);
    // constant values take the constant solver
    let f = k.fillet_variable(&b, &[e], &[vec![2.0, 2.0, 2.0]], 9).unwrap();
    check(&mut k, &f, 7, 15, 6000.0 - (1.0 - PI / 4.0) * 4.0 * 30.0, 1e-6);
}

#[test]
fn fillet_of_new_edges_after_a_union() {
    let mut k = SolidcoreKernel::new();
    let a = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    let b = k.make_box(DVec3::new(5.0, 5.0, 0.0), DVec3::new(15.0, 15.0, 10.0), 2).unwrap();
    let u = k.boolean(BoolOp::Union, &[a, b], 3).unwrap();
    // the two vertical edges where the boxes meet separate a face of each (so do the edges of
    // the merged top and bottom faces, which the engine drops as edges of an operand)
    let es = k.edges(&u, 0).unwrap();
    let mixed: Vec<&EdgeInfo> =
        es.iter().filter(|e| !e.smooth && e.face_tags.len() == 2 && e.face_tags[0] != e.face_tags[1]).collect();
    let new: Vec<u32> = mixed.iter().filter(|e| (e.points[0].z - e.points[1].z).abs() > 1.0).map(|e| e.id).collect();
    assert_eq!(new.len(), 2, "{mixed:?}");
    let f = k.fillet(&u, &new, 1.0, 4).unwrap();
    let tags = check(&mut k, &f, 12, 30, 175.0 * 10.0 + 2.0 * (1.0 - PI / 4.0) * 10.0, 1e-6);
    assert_eq!(tags.iter().filter(|&&t| t == 4).count(), 2, "{tags:?}");
}

#[test]
fn smooth_edges_are_skipped() {
    let mut k = SolidcoreKernel::new();
    let c = k.make_cylinder(10.0, 5.0, 5.0, 1).unwrap();
    let es = k.edges(&c, 0).unwrap();
    let seam: Vec<u32> = es.iter().filter(|e| e.smooth).map(|e| e.id).collect();
    assert_eq!(seam.len(), 1);
    match k.fillet(&c, &seam, 1.0, 9) {
        Err(KernelError::Failed { msg, edges, .. }) => {
            assert!(msg.contains("none of the edges can be blended"), "{msg}");
            assert_eq!(edges, seam);
        }
        r => panic!("{r:?}"),
    }
    // the rims are blended, the seam skipped
    let all: Vec<u32> = es.iter().map(|e| e.id).collect();
    let f = k.fillet(&c, &all, 1.0, 9).unwrap();
    // at both rims a ring of the cross-section between the corner and the arc (area 1 − π/4,
    // its centroid (10 − 3π) / (3 (4 − π)) in from both walls), by Pappus
    let d = (10.0 - 3.0 * PI) / (3.0 * (4.0 - PI));
    let ring = 2.0 * PI * (5.0 - d) * (1.0 - PI / 4.0);
    let exact = PI * 25.0 * 10.0 - 2.0 * ring;
    let m = k.mass_props(&f).unwrap();
    assert!((m.volume - exact).abs() < 1e-5 * exact, "{} vs {exact}", m.volume);
}

#[test]
fn bad_input() {
    let mut k = SolidcoreKernel::new();
    let b = block(&mut k);
    assert!(matches!(k.fillet(&b, &[99], 1.0, 9), Err(KernelError::InvalidInput(_))));
    assert!(matches!(k.fillet(&b, &[], 1.0, 9), Err(KernelError::InvalidInput(_))));
    assert!(matches!(k.fillet(&b, &[0], 0.0, 9), Err(KernelError::InvalidInput(_))));
    assert!(matches!(k.fillet_variable(&b, &[0], &[vec![1.0, -1.0]], 9), Err(KernelError::InvalidInput(_))));
    // much too large: an error, never the unchanged shape
    assert!(k.fillet(&b, &[0], 50.0, 9).is_err());
}

/// A fillet too large for the faces between the edges overlaps itself: `Failed` as the facade's
/// `invalid_faces` (tester finding: `cube(10, $fillet = 6)` and `5.5` were accepted, with a
/// volume that grew with the radius); 4.9 still works and its tiny faces answer `faces`.
#[test]
fn oversize_fillet_overlaps_itself() {
    let mut k = SolidcoreKernel::new();
    let b = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    let all: Vec<u32> = (0..12).collect();
    for r in [6.0, 5.5] {
        match k.fillet(&b, &all, r, 2) {
            Err(KernelError::Failed { msg, edges, .. }) => {
                assert!(msg.contains("overlaps itself"), "{msg}");
                assert!(!edges.is_empty());
            }
            other => panic!("r {r}: {other:?}"),
        }
    }
    let f = k.fillet(&b, &all, 4.9, 2).unwrap();
    assert_eq!(k.faces(&f, 0).unwrap().len(), 26);
    // the exact volume: box minus edges (1 - π/4) r² per length, plus the corner balls
    let r: f64 = 4.9;
    let exact = 1000.0 - 12.0 * (1.0 - PI / 4.0) * r * r * (10.0 - 2.0 * r) - 8.0 * r.powi(3) * (1.0 - PI / 6.0);
    let v = k.mass_props(&f).unwrap().volume;
    assert!((v - exact).abs() < 1e-4 * exact, "{v} vs {exact}");
}

/// Draft mode (plan 1.12): as the facade's `g_draft`, fillets skip the self-overlap and volume
/// checks (results only for a temporary display while a handle is dragged).
#[test]
fn draft_skips_the_checks() {
    let mut k = SolidcoreKernel::new();
    let b = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    let all: Vec<u32> = (0..12).collect();
    k.set_draft(true);
    assert!(k.fillet(&b, &all, 6.0, 2).is_ok());
    k.set_draft(false);
    assert!(k.fillet(&b, &all, 6.0, 2).is_err());
    // the cache estimate grows with the topology
    let f = k.fillet(&b, &all, 1.0, 2).unwrap();
    assert!(k.shape_bytes(&f) > k.shape_bytes(&b));
}
