//! Profiles and sweeps over solidcore (plan 1.1, 1.2, 1.4, 1.5): face and edge counts as the
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

/// 100000 points along `c`.
fn samples(c: &cad_kernel::brep::BSplineCurve) -> Vec<DVec2> {
    let (t0, t1) = c.domain();
    (0..=100_000).map(|i| c.eval(t0 + (t1 - t0) * f64::from(i) / 1e5).truncate()).collect()
}

/// Area and centroid of the closed polyline `p` (counter-clockwise or clockwise).
fn area_centroid(p: &[DVec2]) -> (f64, DVec2) {
    let n = p.len();
    let (mut a, mut c) = (0.0, DVec2::ZERO);
    for i in 0..n {
        let (u, v) = (p[i], p[(i + 1) % n]);
        let w = u.perp_dot(v);
        a += w / 2.0;
        c += (u + v) * w / 6.0;
    }
    (a.abs(), c / a)
}

#[test]
fn extrude_closed_spline() {
    let mut k = SolidcoreKernel::new();
    let pts = vec![DVec2::new(0.0, 0.0), DVec2::new(10.0, 1.0), DVec2::new(12.0, 7.0), DVec2::new(3.0, 9.0)];
    let mut closed = pts.clone();
    closed.push(pts[0]);
    // the same curve as SMLib's (see the unit test in lib.rs): area from a fine polyline
    let c = cad_kernel::curve2::interpolate(&closed, None, None).unwrap();
    let (a, _) = area_centroid(&samples(&c));
    let p = PlacedLoop { lp: Loop::Spline(pts.clone()), frame: DMat4::IDENTITY };
    let s = k.extrude(&p, 2.0, 0.0, DVec2::ONE, 1).unwrap();
    check(&mut k, &s, 3, 3, 2.0 * a, 1e-6);
    // repeated points and a last point equal to the first change nothing
    let mut rep = vec![pts[0], pts[0]];
    rep.extend_from_slice(&pts[1..]);
    rep.push(pts[0]);
    let s = k.extrude(&PlacedLoop { lp: Loop::Spline(rep), frame: DMat4::IDENTITY }, 2.0, 0.0, DVec2::ONE, 1).unwrap();
    check(&mut k, &s, 3, 3, 2.0 * a, 1e-6);
    let two = Loop::Spline(vec![DVec2::ZERO, DVec2::X, DVec2::ZERO]);
    assert!(matches!(
        k.extrude(&PlacedLoop { lp: two, frame: DMat4::IDENTITY }, 2.0, 0.0, DVec2::ONE, 1),
        Err(cad_kernel::KernelError::InvalidInput(_))
    ));
}

#[test]
fn revolve_spline_through_the_axis() {
    let mut k = SolidcoreKernel::new();
    // knob: the closed spline would cross the axis between (0, 20) and (0, 0); it becomes a
    // line along the axis and the open spline through the other points
    let pts = vec![
        DVec2::new(0.0, 0.0),
        DVec2::new(15.0, 0.0),
        DVec2::new(9.0, 10.0),
        DVec2::new(15.0, 20.0),
        DVec2::new(0.0, 20.0),
    ];
    let c = cad_kernel::curve2::interpolate(&pts, None, None).unwrap();
    let (a, cen) = area_centroid(&samples(&c));
    let xz = DMat4::from_cols(DVec3::X.extend(0.0), DVec3::Z.extend(0.0), DVec3::NEG_Y.extend(0.0), glam::DVec4::W);
    let p = PlacedLoop { lp: Loop::Spline(pts), frame: xz };
    let s = k.revolve(&p, TAU, 1).unwrap();
    check(&mut k, &s, 1, 1, TAU * cen.x * a, 1e-6);
    // just off the axis the closed spline runs through: it bulges across (to x = -1.6)
    let bulge = vec![
        DVec2::new(0.001, 0.0),
        DVec2::new(15.0, 0.0),
        DVec2::new(9.0, 10.0),
        DVec2::new(15.0, 20.0),
        DVec2::new(0.001, 20.0),
    ];
    let p = PlacedLoop { lp: Loop::Spline(bulge), frame: xz };
    assert!(matches!(k.revolve(&p, TAU, 1), Err(cad_kernel::KernelError::InvalidInput(m)) if m.contains("axis")));
}

fn square(c: DVec2) -> Loop {
    Loop::Polygon(vec![
        c + DVec2::new(-1.0, -1.0),
        c + DVec2::new(1.0, -1.0),
        c + DVec2::new(1.0, 1.0),
        c + DVec2::new(-1.0, 1.0),
    ])
}

#[test]
fn sweep_straight_and_with_corners() {
    use cad_kernel::{KernelError, PathInterp};
    let mut k = SolidcoreKernel::new();
    let circle = Loop::Circle { center: DVec2::ZERO, r: 1.0 };
    // collinear points: one straight segment
    let path = [DVec3::ZERO, DVec3::new(0.0, 0.0, 5.0), DVec3::new(0.0, 0.0, 10.0)];
    let s = k.sweep(&circle, &path, PathInterp::Linear, 1).unwrap();
    check(&mut k, &s, 3, 3, 10.0 * PI, 1e-6);
    // off-centre profile on a straight path: swept where it lies
    let s = k.sweep(&square(DVec2::ONE), &[DVec3::ZERO, DVec3::new(3.0, 4.0, 0.0)], PathInterp::Linear, 1).unwrap();
    check(&mut k, &s, 6, 12, 20.0, 1e-9);
    // planar polyline with a corner: mitered joint, volume = area × centre line
    let l = [DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 10.0, 0.0)];
    let s = k.sweep(&square(DVec2::ZERO), &l, PathInterp::Linear, 1).unwrap();
    check(&mut k, &s, 8, 18, 80.0, 1e-9);
    let s = k.sweep(&circle, &l, PathInterp::Linear, 1).unwrap();
    check(&mut k, &s, 4, 5, 20.0 * PI, 1e-6);
    // as SMLib: no off-centre profile along corners, no non-planar polyline
    assert!(matches!(k.sweep(&square(DVec2::ONE), &l, PathInterp::Linear, 1), Err(KernelError::Unsupported(_))));
    let bent = [DVec3::ZERO, DVec3::X * 10.0, DVec3::new(10.0, 10.0, 0.0), DVec3::new(10.0, 10.0, 10.0)];
    assert!(matches!(k.sweep(&circle, &bent, PathInterp::Linear, 1), Err(KernelError::Unsupported(_))));
    assert!(matches!(
        k.sweep(&circle, &[DVec3::ZERO, DVec3::ZERO], PathInterp::Linear, 1),
        Err(KernelError::InvalidInput(_))
    ));
}

#[test]
fn sweep_along_splines() {
    use cad_kernel::PathInterp;
    let mut k = SolidcoreKernel::new();
    let circle = Loop::Circle { center: DVec2::ZERO, r: 1.0 };
    // planar spline path: the same curve as curve2::interpolate (SMLib's), volume = area × length
    let pts = [DVec2::ZERO, DVec2::new(10.0, 2.0), DVec2::new(20.0, 0.0), DVec2::new(30.0, 5.0)];
    let c = cad_kernel::curve2::interpolate(&pts, None, None).unwrap();
    let len: f64 = samples(&c).windows(2).map(|w| w[0].distance(w[1])).sum();
    let path: Vec<DVec3> = pts.iter().map(|p| p.extend(0.0)).collect();
    let s = k.sweep(&circle, &path, PathInterp::Spline, 1).unwrap();
    check(&mut k, &s, 3, 3, PI * len, 1e-6);
    let s = k.sweep(&square(DVec2::ZERO), &path, PathInterp::Spline, 1).unwrap();
    check(&mut k, &s, 6, 12, 4.0 * len, 1e-6);
    // helix through 8 points per turn, 4 turns of pitch 10 (the spline is a little shorter
    // than the exact helix)
    let helix: Vec<DVec3> = (0..=32)
        .map(|i| {
            let a = f64::from(i) * TAU / 8.0;
            DVec3::new(10.0 * a.cos(), 10.0 * a.sin(), 10.0 * f64::from(i) / 8.0)
        })
        .collect();
    let exact = PI * 4.0 * (400.0 * PI * PI + 100.0f64).sqrt();
    let s = k.sweep(&circle, &helix, PathInterp::Spline, 1).unwrap();
    check(&mut k, &s, 3, 3, exact, 1e-3);
    let slot = vec![
        line([-1.0, -1.0], [1.0, -1.0]),
        arc([1.0, 0.0], 1.0, -FRAC_PI_2, FRAC_PI_2),
        line([1.0, 1.0], [-1.0, 1.0]),
        arc([-1.0, 0.0], 1.0, FRAC_PI_2, 1.5 * PI),
    ];
    let s = k.sweep(&Loop::Composite(slot), &helix, PathInterp::Spline, 1).unwrap();
    check(&mut k, &s, 6, 12, (4.0 + PI) / PI * exact, 1e-3);
}

#[test]
fn sweep_along_sketch_paths() {
    use cad_kernel::{KernelError, PlacedPath};
    let mut k = SolidcoreKernel::new();
    let circle = Loop::Circle { center: DVec2::ZERO, r: 1.0 };
    // line and tangent quarter arc (solidcore#1450): one B-spline, volume π (10 + 5π)
    let bend = PlacedPath {
        segs: vec![line([0.0, 0.0], [10.0, 0.0]), arc([10.0, 10.0], 10.0, -FRAC_PI_2, 0.0)],
        frame: DMat4::IDENTITY,
    };
    let s = k.sweep_path(&circle, &bend, 1).unwrap();
    check(&mut k, &s, 4, 5, PI * (10.0 + 5.0 * PI), 1e-6);
    let slot = vec![
        line([-1.0, -1.0], [1.0, -1.0]),
        arc([1.0, 0.0], 1.0, -FRAC_PI_2, FRAC_PI_2),
        line([1.0, 1.0], [-1.0, 1.0]),
        arc([-1.0, 0.0], 1.0, FRAC_PI_2, 1.5 * PI),
    ];
    let s = k.sweep_path(&Loop::Composite(slot), &bend, 1).unwrap();
    check(&mut k, &s, 10, 20, (4.0 + PI) * (10.0 + 5.0 * PI), 1e-6);
    // a corner in a placed path: mitered
    let corner = PlacedPath {
        segs: vec![line([0.0, 0.0], [10.0, 0.0]), line([10.0, 0.0], [10.0, 10.0])],
        frame: DMat4::from_rotation_x(FRAC_PI_2),
    };
    let s = k.sweep_path(&square(DVec2::ZERO), &corner, 1).unwrap();
    check(&mut k, &s, 8, 18, 80.0, 1e-9);
    let (lo, hi) = k.bounds(&s).unwrap().unwrap();
    assert!((lo - DVec3::new(0.0, -1.0, -1.0)).abs().max_element() < 1e-6, "{lo}");
    assert!((hi - DVec3::new(11.0, 1.0, 10.0)).abs().max_element() < 1e-6, "{hi}");
    assert!(matches!(k.sweep_path(&square(DVec2::ONE), &corner, 1), Err(KernelError::Unsupported(_))));
}

#[test]
fn extrude_with_twist_and_scale() {
    let mut k = SolidcoreKernel::new();
    // twisted bar: every section is the same square, 12 skin sections
    let s = k.extrude(&placed_loop(square_of(4.0)), 20.0, FRAC_PI_2, DVec2::ONE, 1).unwrap();
    check(&mut k, &s, 6, 12, 1280.0, 1e-5);
    // tapered: frustum h / 3 (A1 + A2 + sqrt(A1 A2)), ruled
    let s = k.extrude(&placed_loop(square_of(6.0)), 10.0, 0.0, DVec2::splat(0.5), 1).unwrap();
    check(&mut k, &s, 6, 12, 840.0, 1e-9);
    let circle = Loop::Circle { center: DVec2::ZERO, r: 5.0 };
    let s = k.extrude(&placed_loop(circle.clone()), 10.0, 0.0, DVec2::splat(0.5), 1).unwrap();
    check(&mut k, &s, 3, 3, PI * 10.0 / 3.0 * (25.0 + 12.5 + 6.25), 1e-6);
    // twist and non-uniform scale, placed: sections scaled after the turn (OpenSCAD's order)
    let rect =
        Loop::Polygon(vec![DVec2::new(-5.0, -3.0), DVec2::new(5.0, -3.0), DVec2::new(5.0, 3.0), DVec2::new(-5.0, 3.0)]);
    let p = PlacedLoop { lp: rect, frame: DMat4::from_translation(DVec3::new(0.0, 0.0, -6.0)) };
    let s = k.extrude(&p, 12.0, -PI / 3.0, DVec2::new(1.0, 0.5), 1).unwrap();
    // the area falls linearly in y: 12 · 60 · (1 + 0.5) / 2
    check(&mut k, &s, 6, 12, 540.0, 1e-5);
    let (lo, hi) = k.bounds(&s).unwrap().unwrap();
    assert!((lo.z + 6.0).abs() < 1e-9 && (hi.z - 6.0).abs() < 1e-9, "{lo} {hi}");
    // a sketch profile and a spline profile turn as curves
    let slot = vec![
        line([-1.0, -1.0], [1.0, -1.0]),
        arc([1.0, 0.0], 1.0, -FRAC_PI_2, FRAC_PI_2),
        line([1.0, 1.0], [-1.0, 1.0]),
        arc([-1.0, 0.0], 1.0, FRAC_PI_2, 1.5 * PI),
    ];
    let s = k.extrude(&placed(slot), 10.0, FRAC_PI_2, DVec2::ONE, 1).unwrap();
    check(&mut k, &s, 6, 12, 10.0 * (4.0 + PI), 1e-5);
    // as SMLib: no apex, no elliptic circle
    for sc in [DVec2::ZERO, DVec2::new(1.0, 0.5)] {
        let r = k.extrude(&placed_loop(circle.clone()), 10.0, 0.0, sc, 1);
        assert!(matches!(r, Err(cad_kernel::KernelError::Unsupported(_))), "{sc}");
    }
}

fn square_of(a: f64) -> Loop {
    Loop::Polygon(vec![DVec2::new(-a, -a), DVec2::new(a, -a), DVec2::new(a, a), DVec2::new(-a, a)])
}

fn placed_loop(lp: Loop) -> PlacedLoop {
    PlacedLoop { lp, frame: DMat4::IDENTITY }
}

fn at(lp: Loop, z: f64) -> PlacedLoop {
    PlacedLoop { lp, frame: DMat4::from_translation(DVec3::new(0.0, 0.0, z)) }
}

#[test]
fn loft_sections() {
    let mut k = SolidcoreKernel::new();
    // two squares: a frustum, ruled or not (two sections are linear either way)
    for ruled in [true, false] {
        let s = k.loft(&[at(square_of(2.0), 0.0), at(square_of(1.0), 6.0)], ruled, 1).unwrap();
        check(&mut k, &s, 6, 12, 56.0, 1e-9);
    }
    // three circles: quadratic in the loft direction, r(t) = 5 - 8 t + 8 t², z = 10 t
    let circle = |r: f64| Loop::Circle { center: DVec2::ZERO, r };
    let secs = [at(circle(5.0), 0.0), at(circle(3.0), 5.0), at(circle(5.0), 10.0)];
    let s = k.loft(&secs, false, 1).unwrap();
    check(&mut k, &s, 3, 3, 138.0 * PI, 1e-5);
    // ruled: two frustums
    let s = k.loft(&secs, true, 1).unwrap();
    check(&mut k, &s, 4, 5, 490.0 * PI / 3.0, 1e-6);
    // sketch sections with the same segment count: segment by segment
    let slot = vec![
        line([-1.0, -1.0], [1.0, -1.0]),
        arc([1.0, 0.0], 1.0, -FRAC_PI_2, FRAC_PI_2),
        line([1.0, 1.0], [-1.0, 1.0]),
        arc([-1.0, 0.0], 1.0, FRAC_PI_2, 1.5 * PI),
    ];
    let s =
        k.loft(&[at(Loop::Composite(slot.clone()), 0.0), at(Loop::Composite(slot.clone()), 5.0)], false, 1).unwrap();
    check(&mut k, &s, 6, 12, 5.0 * (4.0 + PI), 1e-6);
    // different kinds: each section one closed curve (faces and edges as SMLib, volume as
    // SMLib's to 1e-7)
    let s = k.loft(&[at(Loop::Composite(slot), 0.0), at(circle(1.0), 5.0)], false, 1).unwrap();
    check(&mut k, &s, 10, 24, 25.183_72, 1e-6);
    let s = k.loft(&[at(circle(2.0), 0.0), at(square_of(1.5), 6.0)], false, 1).unwrap();
    check(&mut k, &s, 6, 12, 27.925_10, 1e-6);
    assert!(matches!(k.loft(&[at(circle(1.0), 0.0)], false, 1), Err(cad_kernel::KernelError::InvalidInput(_))));
}
