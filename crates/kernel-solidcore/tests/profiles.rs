//! Profiles and sweeps over solidcore (plan 1.1): face and edge counts as the
//! SMLib facade builds them (checked with `mcad diff-kernels`), volumes against the exact value.

#![cfg(feature = "solidcore")]

use cad_kernel::{BoolOp, Kernel, Loop, PlacedLoop, Segment};
use glam::{DMat4, DVec2, DVec3};
use kernel_solidcore::{Shape, SolidcoreKernel};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

fn line(a: [f64; 2], b: [f64; 2]) -> Segment {
    Segment::Line { a: DVec2::from(a), b: DVec2::from(b) }
}

fn arc(c: [f64; 2], r: f64, a0: f64, a1: f64) -> Segment {
    Segment::Arc { center: DVec2::from(c), r, a0, a1 }
}

fn placed(segs: Vec<Segment>) -> PlacedLoop {
    PlacedLoop { lp: Loop::Composite(segs), frame: DMat4::IDENTITY }
}

/// Rectangle `w` × `h` from the origin with corners rounded by `r` (4 lines, 4 quarter arcs).
fn rounded_rect(w: f64, h: f64, r: f64) -> Vec<Segment> {
    vec![
        line([r, 0.0], [w - r, 0.0]),
        arc([w - r, r], r, -FRAC_PI_2, 0.0),
        line([w, r], [w, h - r]),
        arc([w - r, h - r], r, 0.0, FRAC_PI_2),
        line([w - r, h], [r, h]),
        arc([r, h - r], r, FRAC_PI_2, PI),
        line([0.0, h - r], [0.0, r]),
        arc([r, r], r, PI, 1.5 * PI),
    ]
}

/// Faces, edges and volume of `s`; the volume within `rel` of `exact`.
fn check(k: &mut SolidcoreKernel, s: &Shape, faces: usize, edges: usize, exact: f64, rel: f64) {
    let m = k.mass_props(s).unwrap();
    let f = k.faces(s, 0).unwrap().len();
    let e = k.edges(s, 0).unwrap().len();
    assert_eq!((f, e), (faces, edges), "faces/edges, volume {}", m.volume);
    assert!((m.volume - exact).abs() <= rel * exact, "volume {} instead of {exact}", m.volume);
    assert!(k.is_solid(s));
}

#[test]
fn extrude_rounded_rectangle() {
    let mut k = SolidcoreKernel::new();
    let s = k.extrude(&placed(rounded_rect(20.0, 10.0, 2.0)), 5.0, 0.0, DVec2::ONE, 1).unwrap();
    check(&mut k, &s, 10, 24, 5.0 * (200.0 - (4.0 - PI) * 4.0), 1e-8);
    // placed: turned about x, moved, extruded downwards
    let frame = DMat4::from_translation(DVec3::new(5.0, -3.0, 7.0)) * DMat4::from_rotation_x(FRAC_PI_2);
    let p = PlacedLoop { frame, ..placed(rounded_rect(20.0, 10.0, 2.0)) };
    let s = k.extrude(&p, -5.0, 0.0, DVec2::ONE, 1).unwrap();
    check(&mut k, &s, 10, 24, 5.0 * (200.0 - (4.0 - PI) * 4.0), 1e-8);
    let (lo, hi) = k.bounds(&s).unwrap().unwrap();
    assert!((lo - DVec3::new(5.0, -3.0, 7.0)).abs().max_element() < 1e-6, "{lo}");
    assert!((hi - DVec3::new(25.0, 2.0, 17.0)).abs().max_element() < 1e-6, "{hi}");
}

#[test]
fn extrude_slot_and_arcs_only() {
    let mut k = SolidcoreKernel::new();
    // slot: two lines, two half circles
    let slot = vec![
        line([0.0, 0.0], [10.0, 0.0]),
        arc([10.0, 3.0], 3.0, -FRAC_PI_2, FRAC_PI_2),
        line([10.0, 6.0], [0.0, 6.0]),
        arc([0.0, 3.0], 3.0, FRAC_PI_2, 1.5 * PI),
    ];
    let s = k.extrude(&placed(slot), 4.0, 0.0, DVec2::ONE, 1).unwrap();
    check(&mut k, &s, 6, 12, 4.0 * (60.0 + 9.0 * PI), 1e-8);
    // a disc of two half circles, one running clockwise
    let disc = vec![arc([0.0, 0.0], 5.0, 0.0, PI), arc([0.0, 0.0], 5.0, PI, TAU)];
    let s = k.extrude(&placed(disc), 3.0, 0.0, DVec2::ONE, 1).unwrap();
    check(&mut k, &s, 4, 6, 3.0 * 25.0 * PI, 1e-8);
    let cw = vec![arc([0.0, 0.0], 5.0, 0.0, -PI), arc([0.0, 0.0], 5.0, -PI, -TAU)];
    let s = k.extrude(&placed(cw), 3.0, 0.0, DVec2::ONE, 1).unwrap();
    check(&mut k, &s, 4, 6, 3.0 * 25.0 * PI, 1e-8);
    // one full circle as a single arc
    let s = k.extrude(&placed(vec![arc([1.0, 2.0], 5.0, 0.0, TAU)]), 3.0, 0.0, DVec2::ONE, 1).unwrap();
    check(&mut k, &s, 3, 3, 3.0 * 25.0 * PI, 1e-8);
}

#[test]
fn extrude_bspline_segment() {
    let mut k = SolidcoreKernel::new();
    // a quadratic Bézier bulge on top of a 10 × 4 rectangle: area 40 + 2/3 · 10 · 2 (the
    // control point 4 above the chord, the curve 2)
    let bez = Segment::bezier(&[DVec2::new(10.0, 4.0), DVec2::new(5.0, 8.0), DVec2::new(0.0, 4.0)]);
    let segs = vec![line([0.0, 0.0], [10.0, 0.0]), line([10.0, 0.0], [10.0, 4.0]), bez, line([0.0, 4.0], [0.0, 0.0])];
    let s = k.extrude(&placed(segs), 2.0, 0.0, DVec2::ONE, 1).unwrap();
    check(&mut k, &s, 6, 12, 2.0 * (40.0 + 2.0 / 3.0 * 10.0 * 2.0), 1e-8);
}

#[test]
fn extrude_profile_with_hole() {
    let mut k = SolidcoreKernel::new();
    // as the engine does it: the outer loop minus the extruded hole
    let outer = k.extrude(&placed(rounded_rect(20.0, 10.0, 2.0)), 5.0, 0.0, DVec2::ONE, 1).unwrap();
    let hole = vec![arc([10.0, 5.0], 3.0, 0.0, PI), arc([10.0, 5.0], 3.0, PI, TAU)];
    let hole = k.extrude(&placed(hole), 5.0, 0.0, DVec2::ONE, 1).unwrap();
    let s = k.boolean(BoolOp::Difference, &[outer, hole], 1).unwrap();
    check(&mut k, &s, 12, 30, 5.0 * (200.0 - (4.0 - PI) * 4.0 - 9.0 * PI), 1e-8);
}

#[test]
fn revolve_arc_profile() {
    let mut k = SolidcoreKernel::new();
    // half disc right of x = 10 in the xz-plane: Pappus, area π r² / 2 at x = 10 + 4 r / 3π
    let r = 2.0;
    let half = vec![arc([10.0, 0.0], r, -FRAC_PI_2, FRAC_PI_2), line([10.0, r], [10.0, -r])];
    let xz = DMat4::from_cols(DVec3::X.extend(0.0), DVec3::Z.extend(0.0), DVec3::NEG_Y.extend(0.0), glam::DVec4::W);
    let p = PlacedLoop { lp: Loop::Composite(half), frame: xz };
    let ring = PI * r * r / 2.0 * TAU * (10.0 + 4.0 * r / (3.0 * PI));
    let s = k.revolve(&p, TAU, 1).unwrap();
    check(&mut k, &s, 2, 4, ring, 1e-6);
    let s = k.revolve(&p, FRAC_PI_2, 1).unwrap();
    check(&mut k, &s, 4, 6, ring / 4.0, 1e-6);
    // a profile reaching x < 0 crosses the axis
    let across = vec![arc([0.5, 0.0], 1.0, -FRAC_PI_2, FRAC_PI_2), line([0.5, 1.0], [0.5, -1.0])];
    let p = PlacedLoop { lp: Loop::Composite(across), frame: xz };
    let p = PlacedLoop { frame: DMat4::from_translation(DVec3::new(-1.0, 0.0, 0.0)) * p.frame, ..p };
    assert!(matches!(k.revolve(&p, TAU, 1), Err(cad_kernel::KernelError::InvalidInput(_))));
}

#[test]
fn composite_input_errors() {
    let mut k = SolidcoreKernel::new();
    // segments that do not meet
    let gap = vec![line([0.0, 0.0], [1.0, 0.0]), line([1.0, 0.5], [0.0, 1.0]), line([0.0, 1.0], [0.0, 0.0])];
    assert!(matches!(
        k.extrude(&placed(gap), 1.0, 0.0, DVec2::ONE, 1),
        Err(cad_kernel::KernelError::InvalidInput(m)) if m.contains("do not meet")
    ));
    assert!(matches!(
        k.extrude(&placed(vec![arc([0.0, 0.0], 0.0, 0.0, TAU)]), 1.0, 0.0, DVec2::ONE, 1),
        Err(cad_kernel::KernelError::InvalidInput(_))
    ));
}
