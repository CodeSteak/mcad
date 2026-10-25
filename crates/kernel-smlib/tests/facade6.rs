//! Facade version 6: where a Boolean result is not a closed solid (`KernelError::Failed::places`).

mod common;

use cad_kernel::*;
use common::*;
use glam::{DMat4, DVec3};

#[test]
fn facade_version() {
    assert!(k().facade_version() >= 6);
}

/// Cylinder (h 2, centred) minus a torus (tube radius 1) touching its top and bottom faces:
/// no manifold result; the places are the two touching circles (radius 20 at z = ±1).
#[test]
fn tangential_difference_reports_the_touching_circles() {
    let mut k = k();
    let cyl = k.make_cylinder(2.0, 30.0, 30.0, 1).unwrap();
    let cyl = k.transform(&cyl, &DMat4::from_translation(DVec3::new(0.0, 0.0, -1.0))).unwrap();
    let tor = k.make_torus(20.0, 1.0, 2).unwrap();
    let Err(KernelError::Failed { msg, places, .. }) = k.boolean(BoolOp::Difference, &[cyl, tor], 3) else {
        panic!("expected a failure");
    };
    assert!(msg.contains("not a closed solid"), "{msg}");
    assert!(!places.is_empty(), "{msg}");
    for p in places.iter().flatten() {
        let r = p.truncate().length();
        assert!((r - 20.0).abs() < 1e-3 && (p.z.abs() - 1.0).abs() < 1e-6, "{p}: r {r}");
    }
    let zs: Vec<bool> = places.iter().map(|l| l[0].z > 0.0).collect();
    assert!(zs.contains(&true) && zs.contains(&false), "both circles: {places:?}");
}

/// A successful Boolean and other failures carry no places.
#[test]
fn no_places_without_a_contact() {
    let mut k = k();
    let a = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    let b = k.make_box(DVec3::splat(5.0), DVec3::splat(15.0), 2).unwrap();
    assert!(k.boolean(BoolOp::Union, &[a.clone(), b], 3).is_ok());
    if let Err(KernelError::Failed { places, .. }) = k.fillet(&a, &[0], 50.0, 4) {
        assert!(places.is_empty());
    }
}
