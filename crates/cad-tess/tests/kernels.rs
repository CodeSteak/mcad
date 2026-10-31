//! The tessellator on B-reps of both kernels (plan 2.1–2.6): one case per surface kind and per
//! tricky situation. Every mesh must be watertight and consistently oriented outwards, hold the
//! tolerance (measured deviation), enclose the kernel's exact volume within `tol · area`, and
//! carry every face and edge of the B-rep.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use cad_kernel::brep::Surface;
use cad_kernel::{BoolOp, Kernel, Loop, PathInterp, PlacedLoop};
use glam::{DMat4, DVec2, DVec3, DVec4};

const TOL: f64 = 0.05;

struct Case<S> {
    name: &'static str,
    shape: S,
    /// Surface kinds that must occur in the B-rep (the case tests what it claims).
    kinds: &'static [&'static str],
}

/// The cases, built with `k`.
fn cases<K: Kernel>(k: &mut K) -> Vec<Case<K::Shape>> {
    let mut out = Vec::new();
    let mut add = |name, shape: Result<K::Shape, cad_kernel::KernelError>, kinds| match shape {
        Ok(shape) => out.push(Case { name, shape, kinds }),
        Err(e) => panic!("{name}: {e}"),
    };
    let xz = DMat4::from_cols(DVec3::X.extend(0.0), DVec3::Z.extend(0.0), DVec3::NEG_Y.extend(0.0), DVec4::W);
    add("sphere: poles, seam", k.make_sphere(5.0, 1), &["sphere"]);
    add("cylinder: seam", k.make_cylinder(10.0, 4.0, 4.0, 1), &["cylinder"]);
    add("cone with apex", k.make_cylinder(10.0, 0.0, 4.0, 1), &["cone"]);
    add("frustum", k.make_cylinder(10.0, 6.0, 2.0, 1), &["cone"]);
    add("torus: two seams", k.make_torus(10.0, 2.0, 1), &["torus"]);
    let profile = vec![
        DVec2::new(3.0, 0.0),
        DVec2::new(8.0, 0.0),
        DVec2::new(6.0, 6.0),
        DVec2::new(8.0, 12.0),
        DVec2::new(3.0, 12.0),
        DVec2::new(2.5, 6.0),
    ];
    add("revolved spline", k.revolve(&PlacedLoop { lp: Loop::Spline(profile), frame: xz }, TAU, 1), &[]);
    let square =
        Loop::Polygon(vec![DVec2::new(-4.0, -4.0), DVec2::new(4.0, -4.0), DVec2::new(4.0, 4.0), DVec2::new(-4.0, 4.0)]);
    add(
        "twisted extrusion",
        k.extrude(&PlacedLoop { lp: square.clone(), frame: DMat4::IDENTITY }, 20.0, FRAC_PI_2, DVec2::ONE, 1),
        &[],
    );
    add(
        "extruded spline",
        k.extrude(
            &PlacedLoop {
                lp: Loop::Spline(vec![
                    DVec2::new(0.0, 0.0),
                    DVec2::new(8.0, 1.0),
                    DVec2::new(6.0, 6.0),
                    DVec2::new(1.0, 5.0),
                ]),
                frame: DMat4::IDENTITY,
            },
            3.0,
            0.0,
            DVec2::ONE,
            1,
        ),
        &[],
    );
    let bx = k.make_box(DVec3::ZERO, DVec3::new(20.0, 10.0, 5.0), 1).unwrap();
    let mirror = DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0));
    add("mirrored box", k.transform(&bx, &mirror), &[]);
    // a mirror (solidcore's transform may turn the surfaces into B-splines)
    let cyl = k.make_cylinder(10.0, 4.0, 4.0, 1).unwrap();
    let cyl = k.transform(&cyl, &DMat4::from_translation(DVec3::new(5.0, 0.0, 0.0))).unwrap();
    add("mirrored cylinder", k.transform(&cyl, &mirror), &[]);
    // a plate with 5 x 5 holes: many trim loops on two faces
    let plate = k.make_box(DVec3::ZERO, DVec3::new(60.0, 60.0, 4.0), 1).unwrap();
    let mut ops = vec![plate];
    for i in 0..5 {
        for j in 0..5 {
            let c = k.make_cylinder(10.0, 2.0 + 0.3 * (i + j) as f64, 2.0 + 0.3 * (i + j) as f64, 2).unwrap();
            let p = DVec3::new(8.0 + 11.0 * i as f64, 8.0 + 11.0 * j as f64, -3.0);
            ops.push(k.transform(&c, &DMat4::from_translation(p)).unwrap());
        }
    }
    add("plate with 25 holes", k.boolean(BoolOp::Difference, &ops, 3), &["cylinder"]);
    let cube = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    let edges: Vec<u32> = (0..12).collect();
    add("filleted cube", k.fillet(&cube, &edges, 1.5, 4), &["cylinder", "sphere"]);
    let sph = k.make_sphere(10.0, 1).unwrap();
    let cut = k.make_box(DVec3::new(-20.0, -20.0, 4.0), DVec3::new(20.0, 20.0, 30.0), 2).unwrap();
    add("sphere cut below its pole", k.boolean(BoolOp::Difference, &[sph, cut], 3), &["sphere"]);
    // thin slivers: a box and one moved by 1e-3
    let b2 = k.make_box(DVec3::new(0.001, 0.0005, 0.0), DVec3::new(10.001, 10.0005, 10.002), 2).unwrap();
    add("sliver faces", k.boolean(BoolOp::Union, &[cube.clone(), b2], 3), &[]);
    // a tiny face: a 0.02 mm step on a cube
    let step = k.make_box(DVec3::new(2.0, 2.0, 9.98), DVec3::new(2.02, 8.0, 10.5), 2).unwrap();
    add("tiny faces", k.boolean(BoolOp::Union, &[cube.clone(), step], 3), &[]);
    let circle = Loop::Circle { center: DVec2::ZERO, r: 1.5 };
    let path = [DVec3::ZERO, DVec3::new(10.0, 2.0, 3.0), DVec3::new(20.0, 0.0, 0.0), DVec3::new(30.0, 5.0, 4.0)];
    add("swept circle", k.sweep(&circle, &path, PathInterp::Spline, 1), &[]);
    let base = PlacedLoop { lp: square, frame: DMat4::IDENTITY };
    let top = PlacedLoop {
        lp: Loop::Circle { center: DVec2::new(1.0, 0.0), r: 3.0 },
        frame: DMat4::from_translation(DVec3::new(0.0, 0.0, 12.0)),
    };
    add("loft square to circle", k.loft(&[base, top], false, 1), &[]);
    out
}

fn pos(m: &cad_kernel::Mesh, i: u32) -> DVec3 {
    let k = 3 * i as usize;
    DVec3::new(m.positions[k] as f64, m.positions[k + 1] as f64, m.positions[k + 2] as f64)
}

fn run<K: Kernel>(mut k: K, kernel: &str) {
    let mut failures = Vec::new();
    for c in cases(&mut k) {
        let what = format!("{kernel}: {}", c.name);
        let brep = k.brep(&c.shape).unwrap();
        let kinds: Vec<&str> = brep.faces().map(|f| f.surface.kind()).collect();
        for want in c.kinds {
            assert!(kinds.contains(want), "{what}: no {want} in {kinds:?}");
        }
        let t = cad_tess::tessellate_with(&brep, &cad_tess::Options::new(TOL)).unwrap();
        let m = &t.mesh;
        let ch = cad_tess::check(m);
        let exact = k.mass_props(&c.shape).unwrap().volume;
        let mut bad = Vec::new();
        if !t.stats.failed_faces.is_empty() {
            bad.push(format!("failed faces {:?}", t.stats.failed_faces));
        }
        if !ch.watertight() || ch.degenerate > 0 {
            bad.push(format!("{ch:?}"));
        }
        if !ch.volume_ok(exact, TOL) {
            bad.push(format!("volume {} against {exact} (area {})", ch.volume, ch.area));
        }
        if t.stats.max_deviation > TOL * 1.0001 {
            bad.push(format!("deviation {}", t.stats.max_deviation));
        }
        // every face and edge of the B-rep is in the mesh
        for f in brep.faces() {
            if !m.tri_faces.contains(&f.id) {
                bad.push(format!("face {} ({}) without triangles", f.id, f.surface.kind()));
            }
        }
        for e in 0..brep.edges.len() as u32 {
            if !m.edge_ids.contains(&e) {
                bad.push(format!("edge {e} without a polyline"));
            }
        }
        // normals: unit, on the side the triangle faces (outwards: the volume is positive)
        let mut against = 0;
        for tri in m.indices.chunks_exact(3) {
            let n = (pos(m, tri[1]) - pos(m, tri[0])).cross(pos(m, tri[2]) - pos(m, tri[0]));
            let vn: DVec3 = tri
                .iter()
                .map(|&i| {
                    DVec3::new(
                        m.normals[3 * i as usize] as f64,
                        m.normals[3 * i as usize + 1] as f64,
                        m.normals[3 * i as usize + 2] as f64,
                    )
                })
                .sum();
            if n.dot(vn) <= 0.0 {
                against += 1;
            }
        }
        if against > 0 {
            bad.push(format!("{against} triangles against their vertex normals"));
        }
        if m.normals.chunks_exact(3).any(|n| ((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() - 1.0).abs() > 1e-3) {
            bad.push("normals not unit length".into());
        }
        if ch.volume <= 0.0 {
            bad.push("inside out".into());
        }
        eprintln!(
            "{what}: {} triangles, volume {:.4} (exact {exact:.4}, {:.3} of tol·area), deviation {:.4}",
            ch.triangles,
            ch.volume,
            (ch.volume - exact).abs() / (TOL * ch.area),
            t.stats.max_deviation
        );
        if !bad.is_empty() {
            failures.push(format!("{what}: {}", bad.join("; ")));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn smlib_breps() {
    run(kernel_smlib::SmlibKernel::new(), "smlib");
}

#[cfg(feature = "solidcore")]
#[test]
fn solidcore_breps() {
    run(kernel_solidcore::SolidcoreKernel::new(), "solidcore");
}

/// Coarse and fine: the triangle count follows the tolerance, both hold it.
#[test]
fn tolerance_steers_the_density() {
    let mut k = kernel_smlib::SmlibKernel::new();
    let s = k.make_sphere(10.0, 1).unwrap();
    let b = k.brep(&s).unwrap();
    let exact = 4.0 / 3.0 * PI * 1000.0;
    let mut last = 0;
    for tol in [0.5, 0.05, 0.01] {
        let t = cad_tess::tessellate_with(&b, &cad_tess::Options::new(tol)).unwrap();
        let c = cad_tess::check(&t.mesh);
        assert!(c.watertight() && c.volume_ok(exact, tol), "{tol}: {c:?}");
        assert!(t.stats.max_deviation <= tol * 1.0001, "{tol}: {}", t.stats.max_deviation);
        assert!(c.triangles > 2 * last, "{tol}: {} after {last}", c.triangles);
        last = c.triangles;
    }
}

/// Planar faces: projected and triangulated without inner points; holes stay open.
#[test]
fn planar_faces_with_holes() {
    let mut k = kernel_smlib::SmlibKernel::new();
    let a = k.make_box(DVec3::ZERO, DVec3::new(30.0, 20.0, 5.0), 1).unwrap();
    let h = k.make_box(DVec3::new(5.0, 5.0, -1.0), DVec3::new(10.0, 10.0, 6.0), 2).unwrap();
    let h2 = k.make_box(DVec3::new(15.0, 5.0, -1.0), DVec3::new(25.0, 15.0, 6.0), 2).unwrap();
    let s = k.boolean(BoolOp::Difference, &[a, h, h2], 3).unwrap();
    let b = k.brep(&s).unwrap();
    assert!(b.faces().all(|f| matches!(f.surface, Surface::Plane { .. })));
    let t = cad_tess::tessellate_with(&b, &cad_tess::Options::new(TOL)).unwrap();
    let c = cad_tess::check(&t.mesh);
    assert!(c.watertight(), "{c:?}");
    assert_eq!(t.stats.refined, 0);
    assert!((c.volume - (30.0 * 20.0 - 25.0 - 100.0) * 5.0).abs() < 1e-3, "{}", c.volume);
    // a face with n corners (and h holes) has n + 2h - 2 triangles: no inner points
    let top = b.faces().find(|f| f.loops.len() == 3).unwrap();
    let corners: usize =
        top.loops.iter().map(|l| if let cad_kernel::brep::Loop::Edges(e) = l { e.len() } else { 0 }).sum();
    let n = t.mesh.tri_faces.iter().filter(|f| **f == top.id).count();
    assert_eq!(n, corners + 2 * 2 - 2);
}

#[test]
fn errors() {
    let b = cad_kernel::brep::Brep::default();
    assert!(cad_tess::tessellate(&b, 0.0).is_err());
    assert!(cad_tess::tessellate(&b, f64::NAN).is_err());
    assert!(cad_tess::tessellate(&b, 0.1).unwrap().is_empty());
}
