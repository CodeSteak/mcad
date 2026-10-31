//! `nurbs_surface`, `thicken`, `shell`, `offset` over solidcore (plan 1.6), against exact volumes.

#![cfg(feature = "solidcore")]

use cad_kernel::{Kernel, KernelError};
use glam::DVec3;
use kernel_solidcore::SolidcoreKernel;
use std::f64::consts::PI;

fn close(v: f64, exact: f64, rel: f64) {
    assert!((v - exact).abs() <= rel * exact, "volume {v} instead of {exact}");
}

#[test]
fn offset_of_a_box_and_a_sphere() {
    let mut k = SolidcoreKernel::new();
    let b = k.make_box(DVec3::ZERO, DVec3::new(10.0, 20.0, 30.0), 1).unwrap();
    let d = 2.0;
    let o = k.offset(&b, d, 5).unwrap();
    // Minkowski sum with a ball: V + A d + (edge length) π d² / 4 + 4/3 π d³
    close(
        k.mass_props(&o).unwrap().volume,
        6000.0 + 2200.0 * d + 240.0 * PI * d * d / 4.0 + 4.0 / 3.0 * PI * d.powi(3),
        1e-6,
    );
    assert_eq!(k.faces(&o, 0).unwrap().len(), 26);
    // every face is new: the operation's tag
    assert!(k.faces(&o, 0).unwrap().iter().all(|f| f.tag == 5));
    let s = k.make_sphere(5.0, 1).unwrap();
    let o = k.offset(&s, 1.0, 5).unwrap();
    close(k.mass_props(&o).unwrap().volume, 4.0 / 3.0 * PI * 216.0, 1e-6);
    assert!(matches!(k.offset(&b, -1.0, 5), Err(KernelError::InvalidInput(_))));
}

#[test]
fn shells() {
    let mut k = SolidcoreKernel::new();
    let b = k.make_box(DVec3::ZERO, DVec3::new(10.0, 20.0, 30.0), 1).unwrap();
    let faces = k.faces(&b, 0).unwrap();
    let top = faces.iter().find(|f| f.points.iter().all(|p| (p.z - 30.0).abs() < 1e-9)).unwrap().id;
    let s = k.shell(&b, 1.0, &[top], 2).unwrap();
    close(k.mass_props(&s).unwrap().volume, 6000.0 - 8.0 * 18.0 * 29.0, 1e-9);
    assert!(k.is_solid(&s));
    // the outer faces keep the body's tag, the inner ones get the shell's
    let tags: Vec<_> = k.faces(&s, 0).unwrap().iter().map(|f| f.tag).collect();
    assert_eq!(
        (tags.iter().filter(|t| **t == 1).count(), tags.iter().filter(|t| **t == 2).count()),
        (6, 5),
        "{tags:?}"
    );
    let closed = k.shell(&b, 1.0, &[], 2).unwrap();
    close(k.mass_props(&closed).unwrap().volume, 6000.0 - 8.0 * 18.0 * 28.0, 1e-9);
    let sp = k.make_sphere(5.0, 1).unwrap();
    let h = k.shell(&sp, 1.0, &[], 2).unwrap();
    close(k.mass_props(&h).unwrap().volume, 4.0 / 3.0 * PI * (125.0 - 64.0), 1e-6);
    // walls thicker than half the box: no cavity
    assert!(k.shell(&b, 6.0, &[top], 2).is_err());
    assert!(matches!(k.shell(&b, 1.0, &[99], 2), Err(KernelError::InvalidInput(_))));
}

#[test]
fn surface_and_thicken() {
    let mut k = SolidcoreKernel::new();
    let rows: Vec<Vec<DVec3>> =
        (0..4).map(|i| (0..5).map(|j| DVec3::new(f64::from(i), f64::from(j), 0.0)).collect()).collect();
    let s = k.nurbs_surface(&rows, 3).unwrap();
    assert!(!k.is_solid(&s));
    assert_eq!(k.faces(&s, 0).unwrap().len(), 1);
    let m = k.mass_props(&s).unwrap();
    assert_eq!(m.volume, 0.0);
    close(m.area, 12.0, 1e-9);
    let t = k.thicken(&s, 1.0, 4).unwrap();
    assert!(k.is_solid(&t));
    close(k.mass_props(&t).unwrap().volume, 12.0, 1e-9);
    assert_eq!(k.faces(&t, 0).unwrap().len(), 6);
    // a solid cannot be thickened
    let b = k.make_box(DVec3::ZERO, DVec3::ONE, 1).unwrap();
    assert!(matches!(k.thicken(&b, 1.0, 4), Err(KernelError::InvalidInput(_))));
    assert!(matches!(k.nurbs_surface(&rows[..1], 3), Err(KernelError::InvalidInput(_))));
}
