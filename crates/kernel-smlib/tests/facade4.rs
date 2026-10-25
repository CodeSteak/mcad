//! Facade version 4: edge flags, surfaces from few points, variable-radius fillets, shells of
//! rounded bodies.

mod common;

use std::collections::BTreeSet;
use std::f64::consts::PI;

use cad_kernel::*;
use common::*;
use glam::DVec3;

const T_BOX: Tag = 1;
const T_CYL: Tag = 2;
const T_OP: Tag = 50;
const T_FIL: Tag = 77;

#[test]
fn facade_version() {
    assert!(k().facade_version() >= 4);
}

#[test]
fn edges_flag_seams_poles_and_tangent_edges_as_smooth() {
    let mut k = k();
    let cyl = k.make_cylinder(10.0, 5.0, 5.0, T_CYL).unwrap();
    let smooth: Vec<bool> = k.edges(&cyl, 2).unwrap().iter().map(|e| e.smooth).collect();
    assert_eq!(smooth.len(), 3);
    assert_eq!(smooth.iter().filter(|s| **s).count(), 1, "{smooth:?}");
    let sph = k.make_sphere(5.0, T_CYL).unwrap();
    assert!(k.edges(&sph, 2).unwrap().iter().all(|e| e.smooth));
    let b = k.make_box(DVec3::ZERO, DVec3::splat(10.0), T_BOX).unwrap();
    assert!(k.edges(&b, 2).unwrap().iter().all(|e| !e.smooth));
    let ids: Vec<u32> = (0..12).collect();
    let f = k.fillet(&b, &ids, 1.0, T_FIL).unwrap();
    assert!(k.edges(&f, 2).unwrap().iter().all(|e| e.smooth));
}

#[test]
fn surfaces_get_the_degree_their_point_count_allows() {
    let mut k = k();
    for (rows, cols, du, dv) in [(2usize, 2usize, 1u32, 1u32), (3, 3, 2, 2), (2, 5, 1, 3), (5, 2, 3, 1), (4, 4, 3, 3)] {
        let grid: Vec<Vec<DVec3>> = (0..rows)
            .map(|i| {
                (0..cols)
                    .map(|j| {
                        let (x, y) = (20.0 * j as f64 / (cols - 1) as f64, 20.0 * i as f64 / (rows - 1) as f64);
                        DVec3::new(x, y, if (i + j) % 2 == 0 { 0.0 } else { 5.0 })
                    })
                    .collect()
            })
            .collect();
        let s = k.nurbs_surface(&grid, 1).unwrap_or_else(|e| panic!("{rows}x{cols}: {e}"));
        let brep = k.brep(&s).unwrap();
        let f = brep.faces().next().unwrap();
        match &f.surface {
            brep::Surface::BSpline(b) => {
                assert_eq!((b.degree_u, b.degree_v, b.nu, b.nv), (du, dv, rows, cols), "{rows}x{cols}");
            }
            other => panic!("{rows}x{cols}: {}", other.kind()),
        }
        let (lo, hi) = k.bounds(&s).unwrap().unwrap();
        assert!(lo.x.abs() < 1e-9 && (hi.x - 20.0).abs() < 1e-9 && (hi.y - 20.0).abs() < 1e-9, "{lo} {hi}");
    }
}

#[test]
fn law_stations_and_a_linear_law() {
    let mut k = k();
    let b = k.make_box(DVec3::ZERO, DVec3::new(30.0, 20.0, 20.0), T_BOX).unwrap();
    let id = k.edges_where(&b, |e| e.points.iter().all(|p| p.y.abs() < 1e-9 && (p.z - 20.0).abs() < 1e-9)).unwrap()[0];
    let st = k.law_stations(&b, &[id], &[6]).unwrap();
    assert_eq!(st.len(), 1);
    assert_eq!(st[0].len(), 6);
    // the ends of the edge, in between the Greville abscissae of a uniform clamped cubic
    let mut xs: Vec<f64> = st[0].iter().map(|p| p.x).collect();
    assert!(xs[0].min(xs[5]).abs() < 1e-9 && (xs[0].max(xs[5]) - 30.0).abs() < 1e-9, "{xs:?}");
    xs.sort_by(f64::total_cmp);
    for (g, w) in xs.iter().zip([0.0, 10.0 / 3.0, 10.0, 20.0, 80.0 / 3.0, 30.0]) {
        assert!((g - w).abs() < 1e-9, "{xs:?}");
    }
    // radius 1 + x / 15: removes (1 - pi/4) * 130
    let radii: Vec<f64> = st[0].iter().map(|p| 1.0 + p.x / 15.0).collect();
    let f = k.fillet_variable(&b, &[id], &[radii], T_FIL).unwrap();
    assert_approx("linear law", volume(&mut k, &f), 12000.0 - (1.0 - PI / 4.0) * 130.0, 1e-5);
    // equal values: the constant fillet
    let c = k.fillet_variable(&b, &[id], &[vec![2.0; 4]], T_FIL).unwrap();
    let c0 = k.fillet(&b, &[id], 2.0, T_FIL).unwrap();
    assert_approx("constant law", volume(&mut k, &c), volume(&mut k, &c0), 1e-12);
    // a radius <= 0 is invalid input
    let e = k.fillet_variable(&b, &[id], &[vec![1.0, 0.0, 1.0, 1.0]], T_FIL).unwrap_err();
    assert!(matches!(e, KernelError::InvalidInput(_)), "{e:?}");
}

#[test]
fn shell_of_a_rounded_box_open_at_the_top() {
    let mut k = k();
    let b = k.make_box(DVec3::ZERO, DVec3::new(90.0, 34.0, 18.0), T_BOX).unwrap();
    let ids: Vec<u32> = (0..12).collect();
    let f = k.fillet(&b, &ids, 3.0, T_FIL).unwrap();
    let top = k.faces(&f, 1).unwrap().iter().find(|x| x.points.iter().all(|p| (p.z - 18.0).abs() < 1e-6)).unwrap().id;
    let s = k.shell(&f, 1.6, &[top], T_OP).unwrap();
    assert!(k.is_solid(&s));
    let rb = |a: f64, b: f64, c: f64, r: f64| {
        let (x, y, z) = (a - 2.0 * r, b - 2.0 * r, c - 2.0 * r);
        x * y * z + 2.0 * r * (x * y + y * z + x * z) + PI * r * r * (x + y + z) + 4.0 / 3.0 * PI * r.powi(3)
    };
    let want = rb(90.0, 34.0, 18.0, 3.0) - rb(86.8, 30.8, 14.8, 1.4) - 84.0 * 28.0 * 1.6;
    assert_approx("open shell", volume(&mut k, &s), want, 1e-6);
    // outer faces keep their tags (box, rounds), cavity and rim get the operation's
    let tags: BTreeSet<Tag> = k.faces(&s, 1).unwrap().iter().map(|x| x.tag).collect();
    assert!(tags.contains(&T_OP) && tags.contains(&T_FIL) && tags.contains(&T_BOX), "{tags:?}");
    // STEP-ready: the B-rep has one solid with an outer shell
    let brep = k.brep(&s).unwrap();
    assert_eq!(brep.bodies.len(), 1);
    assert!(matches!(&brep.bodies[0], brep::Body::Solid { voids, .. } if voids.is_empty()));
}
