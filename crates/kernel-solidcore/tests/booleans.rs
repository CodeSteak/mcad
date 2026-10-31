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
/// the checks reject it and a retry gives the half sphere.
#[test]
fn tiny_operand() {
    let mut k = SolidcoreKernel::new();
    let b = k.make_box(DVec3::splat(-50.0), DVec3::splat(50.0), 1).unwrap();
    let s = k.make_sphere(0.005, 2).unwrap();
    let s = k.transform(&s, &DMat4::from_translation(DVec3::new(50.0, 0.0, 0.0))).unwrap();
    let half = 2.0 / 3.0 * PI * 0.005f64.powi(3);
    let i = k.boolean(BoolOp::Intersection, &[b.clone(), s.clone()], 3).unwrap();
    let v = k.mass_props(&i).unwrap().volume;
    // (both kernels integrate 2.5818e-7 at this size: 1.4 % below the exact value, not the
    // whole box of 1e6)
    assert!((v - half).abs() < 2e-2 * half, "{v} vs {half}");
    let d = k.boolean(BoolOp::Difference, &[b, s], 3).unwrap();
    let v = k.mass_props(&d).unwrap().volume;
    assert!((v - (1e6 - half)).abs() < 1e-6 * 1e6, "{v}");
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
