//! Kernel fixes from the everyday models (Testbericht 13): transforms keep the tolerances,
//! sweeps of off-centre profiles, plausibility of fillet and Boolean results, mass properties.

mod common;

use cad_kernel::*;
use common::*;
use glam::{DMat4, DVec2, DVec3};

const T: Tag = 1;

/// F8/F7: a moved torus has one vertex. SmBrep::Transform(…, TRUE) derived its tolerances from
/// the vertex box (size 0) and set them to 0; every Boolean then missed the intersections.
#[test]
fn moved_rotated_and_scaled_tori_intersect() {
    let mut k = k();
    let tor = k.make_torus(20.0, 1.0, T).unwrap();
    let vt = 2.0 * PI_ * PI_ * 20.0;
    let slab = k.make_box(DVec3::new(-30.0, -30.0, -5.0), DVec3::new(30.0, 30.0, 0.0), T).unwrap();
    for m in [
        DMat4::IDENTITY,
        DMat4::from_rotation_z(0.5),
        translate(3.0, 0.0, 0.0),
        DMat4::from_rotation_x(std::f64::consts::PI),
    ] {
        let t = k.transform(&tor, &m).unwrap();
        let x = k.boolean(BoolOp::Intersection, &[slab.clone(), t], T).unwrap();
        assert_approx(&format!("lower half of the torus under {m:?}"), volume(&mut k, &x), vt / 2.0, 1e-4);
    }
    // scaled: the tolerances follow the new size
    let big = k.transform(&tor, &DMat4::from_scale(DVec3::splat(1.5))).unwrap();
    let slab2 = k.make_box(DVec3::new(-60.0, -60.0, -5.0), DVec3::new(60.0, 60.0, 0.0), T).unwrap();
    let x = k.boolean(BoolOp::Intersection, &[slab2, big], T).unwrap();
    assert_approx("scaled torus", volume(&mut k, &x), vt * 1.5f64.powi(3) / 2.0, 1e-4);
    // F8a: cylinder plate plus the torus on its top face: one body, not two overlapping ones
    let plate = k.make_cylinder(2.0, 30.0, 30.0, T).unwrap();
    let up = k.transform(&tor, &translate(0.0, 0.0, 2.0)).unwrap();
    let u = k.boolean(BoolOp::Union, &[plate, up], T).unwrap();
    assert_approx("plate u torus", volume(&mut k, &u), PI_ * 900.0 * 2.0 + vt / 2.0, 1e-4);
}

/// F8b: a revolved circle (one vertex as well), moved.
#[test]
fn moved_revolved_ring_unites() {
    let mut k = k();
    let ring = k
        .revolve(
            &PlacedLoop {
                lp: Loop::Circle { center: DVec2::new(20.0, 0.0), r: 1.0 },
                frame: DMat4::from_rotation_x(std::f64::consts::FRAC_PI_2),
            },
            2.0 * std::f64::consts::PI,
            T,
        )
        .unwrap();
    let ring = k.transform(&ring, &translate(0.0, 0.0, 2.0)).unwrap();
    let plate = k.make_cylinder(2.0, 30.0, 30.0, T).unwrap();
    let u = k.boolean(BoolOp::Union, &[plate, ring], T).unwrap();
    assert_approx("plate u ring", volume(&mut k, &u), PI_ * 900.0 * 2.0 + PI_ * PI_ * 20.0, 1e-4);
}

/// F8c: tangential contact along circles has no manifold result: an error, not volume 0.
#[test]
fn tangent_torus_in_a_slab_is_an_error() {
    let mut k = k();
    let slab = k.make_cylinder(2.0, 30.0, 30.0, T).unwrap();
    let slab = k.transform(&slab, &translate(0.0, 0.0, -1.0)).unwrap();
    let tor = k.make_torus(20.0, 1.0, T).unwrap();
    let e = k.boolean(BoolOp::Difference, &[slab, tor], T).unwrap_err();
    assert!(e.to_string().contains("not a closed solid"), "{e}");
}

/// Bodies that only touch along an edge: a − b = a.
#[test]
fn difference_of_bodies_touching_along_an_edge_keeps_the_first() {
    let mut k = k();
    let a = k.make_box(DVec3::ZERO, DVec3::splat(10.0), T).unwrap();
    let b = k.make_box(DVec3::splat(10.0), DVec3::splat(20.0), T).unwrap();
    let b = k.transform(&b, &translate(0.0, 0.0, -10.0)).unwrap(); // shares the edge x = y = 10
    let d = k.boolean(BoolOp::Difference, &[a, b], T).unwrap();
    assert_approx("a - b", volume(&mut k, &d), 1000.0, 1e-6);
}

/// F1: the frame origin of the profile travels along a curved path, centred or not.
#[test]
fn sweep_of_off_centre_and_spline_profiles_along_a_curve() {
    let mut k = k();
    let path = [DVec3::ZERO, DVec3::new(20.0, 0.0, 10.0), DVec3::new(40.0, 0.0, 0.0)];
    let sym =
        Loop::Spline(vec![DVec2::new(5.0, 0.0), DVec2::new(0.0, 3.0), DVec2::new(-5.0, 0.0), DVec2::new(0.0, -3.0)]);
    let s = k.sweep(&sym, &path, PathInterp::Spline, T).unwrap();
    assert!(k.is_solid(&s));
    // circle at (0, 4): its centre runs 4 mm beside the path (y), the body spans y 1..7
    let off = Loop::Circle { center: DVec2::new(0.0, 4.0), r: 3.0 };
    let s = k.sweep(&off, &path, PathInterp::Spline, T).unwrap();
    let (lo, hi) = k.bounds(&s).unwrap().unwrap();
    let c = k.sweep(&Loop::Circle { center: DVec2::ZERO, r: 3.0 }, &path, PathInterp::Spline, T).unwrap();
    let (clo, chi) = k.bounds(&c).unwrap().unwrap();
    // the start frame's x axis is the global x projected, y = z × x: here the profile's y is
    // the model's y (path in the xz plane); the body is the centred one shifted by 4 in y
    assert_approx("y min", lo.y, clo.y + 4.0, 1e-3);
    assert_approx("y max", hi.y, chi.y + 4.0, 1e-3);
    assert_approx("volume", volume(&mut k, &s), volume(&mut k, &c), 1e-3);
}

/// F9: SMLib had no corner type for a vertex and read the vertex's edge list as a corner
/// (segmentation fault); smlib/patches/0002 turns that into an error.
#[test]
fn fillet_at_a_nearly_tangent_circle_fails_without_crash() {
    let mut k = k();
    let pin = k.make_cylinder(8.0, 3.0, 3.0, T).unwrap();
    let pin = k.transform(&pin, &DMat4::from_rotation_x(-std::f64::consts::FRAC_PI_2)).unwrap();
    let path = [DVec3::ZERO, DVec3::new(0.0, -13.0, -1.0), DVec3::new(0.0, -22.0, 4.0), DVec3::new(0.0, -23.0, 10.0)];
    let hook = k.sweep(&Loop::Circle { center: DVec2::ZERO, r: 3.0 }, &path, PathInterp::Spline, 2).unwrap();
    let u = k.boolean(BoolOp::Union, &[pin, hook], T).unwrap();
    // every non-smooth edge where the two bodies meet
    let ids: Vec<u32> = k
        .edges(&u, 4)
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.smooth && e.points.iter().all(|p| p.y.abs() < 0.5))
        .map(|(i, _)| i as u32)
        .collect();
    assert!(!ids.is_empty());
    match k.fillet(&u, &ids, 1.0, T) {
        Ok(s) => assert!(k.is_solid(&s)),
        Err(e) => assert!(e.to_string().contains("fillet failed"), "{e}"),
    }
}

/// F5: a chamfer whose faces collide at a thin corner (SMLib returns an open shell with
/// SM_SUCCESS) is an error; separate chains are tried one after the other.
#[test]
fn chamfer_result_must_be_closed() {
    let mut k = k();
    let outer = k.make_box(DVec3::ZERO, DVec3::new(40.0, 30.0, 50.0), T).unwrap();
    let inner = k.make_box(DVec3::new(3.0, 3.0, 3.0), DVec3::new(37.0, 27.0, 53.0), 2).unwrap();
    let body = k.boolean(BoolOp::Difference, &[outer, inner], T).unwrap();
    let edges = k.edges(&body, 1).unwrap();
    let corner =
        |p: &DVec3| (p.x.abs() < 1e-6 || (p.x - 40.0).abs() < 1e-6) && (p.y.abs() < 1e-6 || (p.y - 30.0).abs() < 1e-6);
    let vert: Vec<u32> =
        edges.iter().enumerate().filter(|(_, e)| e.points.iter().all(corner)).map(|(i, _)| i as u32).collect();
    assert_eq!(vert.len(), 4);
    let r = k.fillet(&body, &vert, 6.0, T).unwrap();
    let top: Vec<u32> = k
        .edges(&r, 1)
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, e)| e.points.iter().all(|p| (p.z - 50.0).abs() < 1e-6))
        .map(|(i, _)| i as u32)
        .collect();
    assert_eq!(top.len(), 12);
    let e = k.chamfer(&r, &top, 0.8, T).unwrap_err();
    assert!(e.to_string().contains("not closed"), "{e}");
    let ok = k.chamfer(&r, &top, 0.7, T).unwrap();
    assert!(k.is_solid(&ok));
    let v0 = volume(&mut k, &r);
    let v1 = volume(&mut k, &ok);
    assert!(v1 < v0 && v1 > v0 - 200.0, "{v0} -> {v1}");
}

/// Mass properties: volume relative accuracy about 1e-6 (was 1e-8, which took 70 % of an
/// evaluation of the Schlüsselbrett).
#[test]
fn volume_accuracy() {
    let mut k = k();
    let s = k.make_sphere(5.0, T).unwrap();
    assert_approx("sphere", volume(&mut k, &s), 4.0 / 3.0 * PI_ * 125.0, 1e-5);
    let t = k.make_torus(20.0, 1.0, T).unwrap();
    assert_approx("torus", volume(&mut k, &t), 2.0 * PI_ * PI_ * 20.0, 1e-5);
    let c = k.make_cylinder(10.0, 3.0, 1.0, T).unwrap();
    assert_approx("cone", volume(&mut k, &c), PI_ * 10.0 / 3.0 * 13.0, 1e-5);
}
