//! Facade version 5: outward offset (Minkowski sum with a ball), exact or an error.

mod common;

use std::collections::BTreeMap;
use std::f64::consts::PI;

use cad_kernel::*;
use common::*;
use glam::{DMat4, DVec2, DVec3};
use kernel_smlib::{Shape, SmlibKernel};

const T_IN: Tag = 1;
const T_OFF: Tag = 9;

fn surface_kinds(k: &mut SmlibKernel, s: &Shape) -> BTreeMap<&'static str, usize> {
    let mut m = BTreeMap::new();
    for f in k.brep(s).unwrap().faces() {
        *m.entry(f.surface.kind()).or_default() += 1;
    }
    m
}

fn kinds(list: &[(&'static str, usize)]) -> BTreeMap<&'static str, usize> {
    list.iter().copied().collect()
}

#[test]
fn facade_version() {
    assert!(k().facade_version() >= 5);
}

#[test]
fn box_offset_is_the_rounded_box() {
    let mut k = k();
    let (a, b, c, r) = (10.0, 20.0, 30.0, 2.0);
    let bx = k.make_box(DVec3::ZERO, DVec3::new(a, b, c), T_IN).unwrap();
    let o = k.offset(&bx, r, T_OFF).unwrap();
    let want = a * b * c + 2.0 * r * (a * b + b * c + c * a) + PI * r * r * (a + b + c) + 4.0 / 3.0 * PI * r.powi(3);
    assert_approx("volume", volume(&mut k, &o), want, 1e-6);
    assert_eq!(surface_kinds(&mut k, &o), kinds(&[("cylinder", 12), ("plane", 6), ("sphere", 8)]));
    let (lo, hi) = k.bounds(&o).unwrap().unwrap();
    assert!(lo.abs_diff_eq(DVec3::splat(-r), 1e-9) && hi.abs_diff_eq(DVec3::new(a + r, b + r, c + r), 1e-9));
    // all faces are new: the operation's tag
    assert!(k.faces(&o, 1).unwrap().iter().all(|f| f.tag == T_OFF));
    // rotated: same body
    let rb = k.transform(&bx, &(DMat4::from_rotation_x(0.3) * DMat4::from_rotation_z(0.5))).unwrap();
    let ro = k.offset(&rb, r, T_OFF).unwrap();
    assert_approx("volume rotated", volume(&mut k, &ro), want, 1e-6);
}

#[test]
fn cylinder_sphere_and_rounded_box() {
    let mut k = k();
    let r = 2.0;
    let (h, rc) = (10.0, 5.0);
    let cy = k.make_cylinder(h, rc, rc, T_IN).unwrap();
    let o = k.offset(&cy, r, T_OFF).unwrap();
    // Steiner: V + S r + M r^2 + 4/3 pi r^3 with M = pi h + pi^2 R
    let s = 2.0 * PI * rc * h + 2.0 * PI * rc * rc;
    let want = PI * rc * rc * h + s * r + (PI * h + PI * PI * rc) * r * r + 4.0 / 3.0 * PI * r.powi(3);
    assert_approx("cylinder", volume(&mut k, &o), want, 1e-6);
    let sp = k.make_sphere(5.0, T_IN).unwrap();
    let o = k.offset(&sp, r, T_OFF).unwrap();
    assert_approx("sphere", volume(&mut k, &o), 4.0 / 3.0 * PI * 7.0f64.powi(3), 1e-6);
    assert_eq!(surface_kinds(&mut k, &o), kinds(&[("sphere", 1)]));
    // rounded box (r 1) + 2 = box shrunk by 1, offset by 3
    let bx = k.make_box(DVec3::ZERO, DVec3::new(10.0, 20.0, 30.0), T_IN).unwrap();
    let ids: Vec<u32> = (0..12).collect();
    let fb = k.fillet(&bx, &ids, 1.0, T_IN).unwrap();
    let o = k.offset(&fb, r, T_OFF).unwrap();
    let (a, b, c, r3) = (8.0, 18.0, 28.0, 3.0);
    let want =
        a * b * c + 2.0 * r3 * (a * b + b * c + c * a) + PI * r3 * r3 * (a + b + c) + 4.0 / 3.0 * PI * r3.powi(3);
    assert_approx("rounded box", volume(&mut k, &o), want, 1e-6);
}

#[test]
fn concave_edges_stay_sharp() {
    let mut k = k();
    let r = 2.0;
    let l = Loop::Polygon(vec![
        DVec2::new(0.0, 0.0),
        DVec2::new(20.0, 0.0),
        DVec2::new(20.0, 5.0),
        DVec2::new(5.0, 5.0),
        DVec2::new(5.0, 20.0),
        DVec2::new(0.0, 20.0),
    ]);
    let ls = k.extrude(&PlacedLoop { lp: l, frame: DMat4::IDENTITY }, 10.0, 0.0, DVec2::ONE, T_IN).unwrap();
    let o = k.offset(&ls, r, T_OFF).unwrap();
    // cross-sections: L + disc(rho), area A + P rho + (5 pi / 4 - 1) rho^2 (5 convex corners, 1 concave)
    let (a, p, h, q) = (175.0, 80.0, 10.0, 5.0 * PI / 4.0 - 1.0);
    let want = h * (a + p * r + q * r * r) + 2.0 * (a * r + p * PI * r * r / 4.0 + q * 2.0 * r.powi(3) / 3.0);
    assert_approx("L", volume(&mut k, &o), want, 1e-6);
}

#[test]
fn inexact_corners_are_errors() {
    let mut k = k();
    let pts = [DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0), DVec3::new(0.0, 10.0, 0.0), DVec3::new(0.0, 0.0, 10.0)];
    let tet = k.make_polyhedron(&pts, &[vec![0, 2, 1], vec![0, 1, 3], vec![0, 3, 2], vec![1, 2, 3]], T_IN).unwrap();
    let e = k.offset(&tet, 2.0, T_OFF).unwrap_err();
    assert!(e.to_string().contains("not exact"), "{e}");
    let bx = k.make_box(DVec3::ZERO, DVec3::ONE, T_IN).unwrap();
    assert!(matches!(k.offset(&bx, 0.0, T_OFF), Err(KernelError::InvalidInput(_))));
    let sheet =
        k.nurbs_surface(&[vec![DVec3::ZERO, DVec3::X], vec![DVec3::Y, DVec3::new(1.0, 1.0, 0.0)]], T_IN).unwrap();
    assert!(k.offset(&sheet, 1.0, T_OFF).is_err());
}
