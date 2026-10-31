//! Booleans with the facade's result checks (re-test of task C): no silent wrong results.

#![cfg(feature = "solidcore")]

use cad_kernel::{BoolOp, Kernel, KernelError};
use glam::{DMat4, DVec3};
use kernel_solidcore::SolidcoreKernel;
use std::f64::consts::PI;

/// A box minus a cylinder that touches one of its faces tangentially: SMLib and solidcore give
/// no closed solid; `Failed` with the facade's wording instead of an open body.
#[test]
fn tangential_difference_fails() {
    let mut k = SolidcoreKernel::new();
    let b = k.make_box(DVec3::new(-6.0, -12.0, 0.0), DVec3::new(0.0, -2.0, 4.0), 1).unwrap();
    let c = k.make_cylinder(10.0, 1.0, 1.0, 2).unwrap();
    let c = k.transform(&c, &DMat4::from_translation(DVec3::new(-3.0, -3.0, -1.0))).unwrap();
    match k.boolean(BoolOp::Difference, &[b, c], 3) {
        Err(KernelError::Failed { msg, .. }) => assert!(msg.contains("not a closed solid"), "{msg}"),
        other => panic!("{other:?}"),
    }
}

/// A box and a sphere 1e-4 of its size at its face: solidcore's first result is the whole box;
/// the checks reject it.
#[test]
fn tiny_operand() {
    let mut k = SolidcoreKernel::new();
    let b = k.make_box(DVec3::splat(-50.0), DVec3::splat(50.0), 1).unwrap();
    let s = k.make_sphere(0.005, 2).unwrap();
    let s = k.transform(&s, &DMat4::from_translation(DVec3::new(50.0, 0.0, 0.0))).unwrap();
    let half = 2.0 / 3.0 * PI * 0.005f64.powi(3);
    // the half sphere, or a loud failure: before task D's tester findings a retry moved the
    // sphere by 1e-4 of the pair's size, three times its radius, and passed
    let wrong = |e: &KernelError| e.to_string().contains("wrong result");
    match k.boolean(BoolOp::Intersection, &[b.clone(), s.clone()], 3) {
        Ok(i) => {
            let v = k.mass_props(&i).unwrap().volume;
            // (both kernels integrate 2.5818e-7 at this size: 1.4 % below the exact value)
            assert!((v - half).abs() < 2e-2 * half, "{v} vs {half}");
        }
        Err(e) => assert!(wrong(&e), "{e}"),
    }
    match k.boolean(BoolOp::Difference, &[b, s], 3) {
        Ok(d) => {
            let v = k.mass_props(&d).unwrap().volume;
            assert!((v - (1e6 - half)).abs() < 1e-6 * 1e6, "{v}");
        }
        Err(e) => assert!(wrong(&e), "{e}"),
    }
}

/// Bodies that touch along a face: a − b = a, a ∩ b = empty (the facade's rule), a ∪ b one body.
#[test]
fn touching_bodies() {
    let mut k = SolidcoreKernel::new();
    let a = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    let b = k.make_box(DVec3::new(10.0, 0.0, 0.0), DVec3::new(20.0, 10.0, 10.0), 2).unwrap();
    let d = k.boolean(BoolOp::Difference, &[a.clone(), b.clone()], 3).unwrap();
    assert!((k.mass_props(&d).unwrap().volume - 1000.0).abs() < 1e-9);
    let i = k.boolean(BoolOp::Intersection, &[a.clone(), b.clone()], 3).unwrap();
    assert!(k.is_empty(&i) || k.mass_props(&i).unwrap().volume.abs() < 1e-9);
    let u = k.boolean(BoolOp::Union, &[a, b], 3).unwrap();
    assert!((k.mass_props(&u).unwrap().volume - 2000.0).abs() < 1e-9);
}

/// Boxes that touch along an edge: the union is one closed solid with a shell per box (the
/// facade's `touching_union`), not solidcore's non-manifold body.
#[test]
fn union_of_bodies_touching_along_an_edge() {
    let mut k = SolidcoreKernel::new();
    let a = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    let b = k.make_box(DVec3::new(10.0, 10.0, 0.0), DVec3::new(20.0, 20.0, 10.0), 2).unwrap();
    let u = k.boolean(BoolOp::Union, &[a, b], 3).unwrap();
    assert!(k.is_solid(&u));
    assert!((k.mass_props(&u).unwrap().volume - 2000.0).abs() < 1e-9);
    assert_eq!(k.faces(&u, 0).unwrap().len(), 12);
}

/// A sphere inside a box, away from its faces (tester finding on task D: no point was checked,
/// only the 3 % volume bound guarded): every operation gives the exact volume.
#[test]
fn operand_inside_the_other() {
    let mut k = SolidcoreKernel::new();
    let b = k.make_box(DVec3::splat(-50.0), DVec3::splat(50.0), 1).unwrap();
    let s = k.make_sphere(5.0, 2).unwrap();
    let vs = 4.0 / 3.0 * PI * 125.0;
    for (op, want) in [(BoolOp::Union, 1e6), (BoolOp::Difference, 1e6 - vs), (BoolOp::Intersection, vs)] {
        let r = k.boolean(op, &[b.clone(), s.clone()], 3).unwrap();
        let v = k.mass_props(&r).unwrap().volume;
        assert!((v - want).abs() < 1e-6 * want, "{op:?}: {v} against {want}");
    }
}

/// Operands far smaller than 1e-4 of the pair (tester finding on task D): the checks' offset
/// and the retries' shift follow the smaller operand, so it is neither moved nor lost silently.
#[test]
fn operand_smaller_than_the_offset() {
    let mut k = SolidcoreKernel::new();
    let big = k.make_box(DVec3::splat(-5000.0), DVec3::splat(5000.0), 1).unwrap();
    let s = k.make_sphere(0.005, 2).unwrap();
    let s = k.transform(&s, &DMat4::from_translation(DVec3::new(5000.0, 0.0, 0.0))).unwrap();
    if let Ok(r) = k.boolean(BoolOp::Union, &[big, s], 3) {
        let (_, hi) = k.bounds(&r).unwrap().unwrap();
        assert!((hi.x - 5000.005).abs() < 1e-5, "sphere moved: {}", hi.x);
    }
    let cube = k.make_box(DVec3::splat(-500.0), DVec3::splat(500.0), 1).unwrap();
    let s = k.make_sphere(0.002, 2).unwrap();
    let s = k.transform(&s, &DMat4::from_translation(DVec3::new(499.999, 0.0, 0.0))).unwrap();
    if let Ok(r) = k.boolean(BoolOp::Difference, &[cube, s], 3) {
        assert!(k.faces(&r, 0).unwrap().len() > 6, "pocket lost");
    }
    let sph = k.make_sphere(1000.0, 1).unwrap();
    let c = k.make_box(DVec3::splat(-0.005), DVec3::splat(0.005), 2).unwrap();
    let c = k.transform(&c, &DMat4::from_translation(DVec3::new(1000.0, 0.0, 0.0))).unwrap();
    if let Ok(r) = k.boolean(BoolOp::Intersection, &[sph, c], 3) {
        // not the whole cube: it ends at the sphere (the volume of so small a piece of a large
        // sphere is beyond the integration's accuracy: 1e-6 instead of 0.5e-6)
        let (_, hi) = k.bounds(&r).unwrap().unwrap();
        assert!(hi.x < 1000.0 + 1e-6, "whole cube: {}", hi.x);
    }
}
