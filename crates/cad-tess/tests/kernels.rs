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

/// A mesh within `tol` of the B-rep of `s`: strict (no face above the tolerance), watertight,
/// the exact volume within `tol · area`.
fn holds<K: Kernel>(k: &mut K, s: &K::Shape, tol: f64, what: &str) -> usize {
    let brep = k.brep(s).unwrap();
    let t = cad_tess::tessellate_with(&brep, &cad_tess::Options::new(tol)).unwrap();
    if let Err(e) = t.strict() {
        panic!("{what}: {e}");
    }
    let c = cad_tess::check(&t.mesh);
    let exact = k.mass_props(s).unwrap().volume;
    assert!(c.watertight() && c.degenerate == 0, "{what}: {c:?}");
    assert!(c.volume_ok(exact, tol), "{what}: volume {} against {exact}", c.volume);
    assert!(t.stats.max_deviation <= tol * 1.0001, "{what}: deviation {}", t.stats.max_deviation);
    c.triangles
}

/// Ruled bands (a revolved straight line, a cone, a cylinder) cut by Booleans at several
/// scales: the case of the mug's wall where the handle joins it (straight along the profile,
/// rows of the grid only near the holes), whose refinement did not converge at 0.01 once the
/// mug was scaled by 2 (0.057 mm left after 40 passes).
fn ruled_bands<K: Kernel>(mut k: K, kernel: &str) {
    let xz = DMat4::from_cols(DVec3::X.extend(0.0), DVec3::Z.extend(0.0), DVec3::NEG_Y.extend(0.0), DVec4::W);
    for (s, tols) in [(1.0, &[0.01, 0.005][..]), (2.0, &[0.01][..]), (4.0, &[0.01][..])] {
        let profile = Loop::Polygon(vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(36.0 * s, 0.0),
            DVec2::new(41.0 * s, 95.0 * s),
            DVec2::new(0.0, 95.0 * s),
        ]);
        let walls = [
            ("revolved line", k.revolve(&PlacedLoop { lp: profile, frame: xz }, TAU, 1).unwrap()),
            ("cone", k.make_cylinder(95.0 * s, 36.0 * s, 41.0 * s, 1).unwrap()),
            ("cylinder", k.make_cylinder(95.0 * s, 38.0 * s, 38.0 * s, 1).unwrap()),
        ];
        for (name, wall) in walls {
            // two stubs into the wall at an angle (a handle's ends) and a hole through it
            let stub = |k: &mut K, z: f64, tilt: f64| {
                let c = k.make_cylinder(30.0 * s, 4.5 * s, 4.5 * s, 2).unwrap();
                let m = DMat4::from_translation(DVec3::new(-38.0 * s - 20.0 * s, 0.0, z * s))
                    * DMat4::from_rotation_y(FRAC_PI_2 + tilt);
                k.transform(&c, &m).unwrap()
            };
            let (a, b) = (stub(&mut k, 22.0, 0.15), stub(&mut k, 77.0, -0.15));
            let body = k.boolean(BoolOp::Union, &[wall, a, b], 3).unwrap();
            let hole = k.make_cylinder(100.0 * s, 6.0 * s, 6.0 * s, 4).unwrap();
            let hole = k
                .transform(
                    &hole,
                    &(DMat4::from_translation(DVec3::new(0.0, 50.0 * s, 50.0 * s)) * DMat4::from_rotation_x(FRAC_PI_2)),
                )
                .unwrap();
            let body = k.boolean(BoolOp::Difference, &[body, hole], 3).unwrap();
            for &tol in tols {
                let n = holds(&mut k, &body, tol, &format!("{kernel}: {name} ×{s} at {tol}"));
                eprintln!("{kernel}: {name} ×{s} at {tol}: {n} triangles");
            }
        }
    }
}

#[test]
fn ruled_bands_smlib() {
    ruled_bands(kernel_smlib::SmlibKernel::new(), "smlib");
}

#[cfg(feature = "solidcore")]
#[test]
fn ruled_bands_solidcore() {
    ruled_bands(kernel_solidcore::SolidcoreKernel::new(), "solidcore");
}

/// The mug of the corpus scaled by 2 at 0.01 (its conical wall, see `ruled_bands`), and at
/// 0.005 unscaled.
fn scaled_mug<K: Kernel + 'static>(k: K, kernel: &str)
where
    K::Shape: 'static,
{
    use cad_session::protocol::ToEngine;
    use cad_session::{Session, SessionOptions, std_clock};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/models/everyday");
    let text = std::fs::read_to_string(root.join("mug.mcad")).unwrap();
    assert!(text.contains("\nb: mug();"));
    let opts = SessionOptions { auto_root_mesh: false, ..Default::default() };
    let mut s = Session::new(k, opts, std_clock());
    s.set_resolver(Box::new(cad_session::FsResolver { root }), "mug.mcad");
    for (version, (inst, tol)) in [("scale(2) mug();", 0.01), ("mug();", 0.005)].into_iter().enumerate() {
        let version = version as u64 + 1;
        s.post(ToEngine::SetSource { text: text.replace("\nb: mug();", &format!("\n{inst}")), version });
        s.run_to_idle();
        let d = s.done().unwrap();
        let r = d.ev.root().unwrap();
        let parts: Vec<_> = d.ev.parts(r).iter().map(|p| p.shape.clone()).collect();
        assert_eq!(parts.len(), 1, "{kernel}: {inst}");
        let k = s.engine_mut().kernel_mut();
        let n = holds(k, &parts[0], tol, &format!("{kernel}: {inst} at {tol}"));
        eprintln!("{kernel}: {inst} at {tol}: {n} triangles");
    }
}

#[test]
fn scaled_mug_smlib() {
    scaled_mug(kernel_smlib::SmlibKernel::new(), "smlib");
}

#[cfg(feature = "solidcore")]
#[test]
fn scaled_mug_solidcore() {
    scaled_mug(kernel_solidcore::SolidcoreKernel::new(), "solidcore");
}

/// Triangles facing against the mean of their vertex normals (the surface's: folds) and the
/// largest angle between a triangle and the normal at one of its corners, degrees.
fn folds(m: &cad_kernel::Mesh) -> (usize, f64) {
    let n = |i: u32| {
        let k = 3 * i as usize;
        DVec3::new(m.normals[k] as f64, m.normals[k + 1] as f64, m.normals[k + 2] as f64)
    };
    let (mut folds, mut worst) = (0, 0f64);
    for t in m.indices.chunks_exact(3) {
        let Some(tn) = (pos(m, t[1]) - pos(m, t[0])).cross(pos(m, t[2]) - pos(m, t[0])).try_normalize() else {
            continue;
        };
        if (n(t[0]) + n(t[1]) + n(t[2])).normalize_or_zero().dot(tn) < 0.0 {
            folds += 1;
        }
        for &i in t {
            if let Some(v) = n(i).try_normalize() {
                worst = worst.max(v.dot(tn).clamp(-1.0, 1.0).acos().to_degrees());
            }
        }
    }
    (folds, worst)
}

/// A ruled band with many holes (a cone between two radii, 7 rings of 18 radial holes): the
/// rows of the straight direction stepped over most of the rings, and the CDT, in coordinates
/// that are not affine there, made triangles that faced against the surface at the holes' rims
/// (24 at 0.01, exported). Zero folds at 0.01 and 0.005.
fn band_holes<K: Kernel>(mut k: K, kernel: &str) {
    let outer = k.make_cylinder(100.0, 40.0, 45.0, 1).unwrap();
    let inner = k.make_cylinder(100.0, 37.0, 42.0, 1).unwrap();
    let inner = k.transform(&inner, &DMat4::from_translation(DVec3::new(0.0, 0.0, 3.0))).unwrap();
    let mut ops = vec![outer, inner];
    for z in (10..=58).step_by(8) {
        for a in (0..=340).step_by(20) {
            let c = k.make_cylinder(20.0, 2.5, 2.5, 2).unwrap();
            let m = DMat4::from_rotation_z(((a + z) as f64).to_radians())
                * DMat4::from_translation(DVec3::new(40.0, 0.0, z as f64))
                * DMat4::from_rotation_y(FRAC_PI_2)
                * DMat4::from_translation(DVec3::new(0.0, 0.0, -10.0));
            ops.push(k.transform(&c, &m).unwrap());
        }
    }
    let body = k.boolean(BoolOp::Difference, &ops, 3).unwrap();
    for tol in [0.01, 0.005] {
        let what = format!("{kernel}: band with holes at {tol}");
        holds(&mut k, &body, tol, &what);
        let m = cad_tess::tessellate(&k.brep(&body).unwrap(), tol).unwrap();
        let (f, worst) = folds(&m);
        assert_eq!(f, 0, "{what}: {f} folds");
        assert!(worst < 60.0, "{what}: a triangle {worst:.1}° off a corner's normal");
    }
}

#[test]
fn band_holes_smlib() {
    band_holes(kernel_smlib::SmlibKernel::new(), "smlib");
}

#[cfg(feature = "solidcore")]
#[test]
fn band_holes_solidcore() {
    band_holes(kernel_solidcore::SolidcoreKernel::new(), "solidcore");
}

/// `f` on the kernel and the evaluated root part of a model file of the corpus
/// (`tests/models/…`), or of `src` in its place.
fn with_part<K: Kernel + 'static, R>(k: K, file: &str, src: Option<&str>, f: impl FnOnce(&mut K, &K::Shape) -> R) -> R
where
    K::Shape: 'static,
{
    use cad_session::protocol::ToEngine;
    use cad_session::{Session, SessionOptions, std_clock};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/models").join(file);
    let text = match src {
        Some(s) => s.to_string(),
        None => std::fs::read_to_string(&path).unwrap(),
    };
    let opts = SessionOptions { auto_root_mesh: false, ..Default::default() };
    let mut s = Session::new(k, opts, std_clock());
    let main = path.file_name().unwrap().to_string_lossy().into_owned();
    s.set_resolver(Box::new(cad_session::FsResolver { root: path.parent().unwrap().to_path_buf() }), &main);
    s.post(ToEngine::SetSource { text, version: 1 });
    s.run_to_idle();
    let d = s.done().unwrap();
    let r = d.ev.root().unwrap();
    let parts: Vec<_> = d.ev.parts(r).iter().map(|p| p.shape.clone()).collect();
    assert_eq!(parts.len(), 1, "{file}");
    f(s.engine_mut().kernel_mut(), &parts[0])
}

/// Corner blends that taper to a needle at a pole (`pillow_block`, `soap_dish` at 0.01): a ring
/// of grid points nearer the pole than the boundary's first point left triangles from the pole
/// to points of two heights, up to 86° off the surface. Now within 60° of every corner.
/// A crescent whose arcs touch (`graze2` of the tester: a cylinder minus one shifted by 0.02):
/// both faces at the cusp meshed the same sliver, a fin of two triangles facing each other
/// (an edge used four times). Watertight, no duplicate triangles.
fn tapered_and_grazing<K: Kernel + 'static, F: Fn() -> K>(new: F, kernel: &str)
where
    K::Shape: 'static,
{
    for file in ["everyday/pillow_block.mcad", "everyday/soap_dish.mcad"] {
        with_part(new(), file, None, |k, s| {
            holds(k, s, 0.01, &format!("{kernel}: {file}"));
            let (f, worst) = folds(&cad_tess::tessellate(&k.brep(s).unwrap(), 0.01).unwrap());
            assert!(f == 0 && worst < 60.0, "{kernel}: {file}: {f} folds, {worst:.1}°");
        });
    }
    let src = "difference() { cylinder(r = 30, h = 60); translate([0, 0.02, 0]) cylinder(r = 30, h = 30); }\n";
    with_part(new(), "spec/graze.mcad", Some(src), |k, s| {
        for tol in [0.05, 0.01] {
            let m = cad_tess::tessellate(&k.brep(s).unwrap(), tol).unwrap();
            let c = cad_tess::check(&m);
            assert!(c.watertight() && c.duplicate_triangles == 0, "{kernel}: graze at {tol}: {c:?}");
        }
    });
}

#[test]
fn tapered_and_grazing_smlib() {
    tapered_and_grazing(kernel_smlib::SmlibKernel::new, "smlib");
}

#[cfg(feature = "solidcore")]
#[test]
fn tapered_and_grazing_solidcore() {
    tapered_and_grazing(kernel_solidcore::SolidcoreKernel::new, "solidcore");
}

/// Flips that repair folds or skewed triangles keep the tolerance: `f12`'s twisted loft at 0.01
/// (a flip across the curved surface put the new edge 0.0136 mm off it) and a twisted bar at
/// 0.05 (0.0521), measured independently of the tessellator's own figures by sampling.
fn flips_keep_the_tolerance<K: Kernel + 'static, F: Fn() -> K>(new: F, kernel: &str)
where
    K::Shape: 'static,
{
    let twist = "linear_extrude(height = 30, twist = 360) square([60, 2], center = true);\n";
    for (file, src, tol) in
        [("../everyday/bugs/f12_shell_loft_square_circle.mcad", None, 0.01), ("spec/twist.mcad", Some(twist), 0.05)]
    {
        with_part(new(), file, src, |k, s| {
            holds(k, s, tol, &format!("{kernel}: {file} at {tol}"));
            let brep = k.brep(s).unwrap();
            let m = cad_tess::tessellate(&brep, tol).unwrap();
            let d = cad_tess::surface_distance(&brep, &m);
            assert!(d.chord.max <= tol * 1.001, "{kernel}: {file} at {tol}: chord {} at {}", d.chord.max, d.chord.at);
        });
    }
}

#[test]
fn flips_keep_the_tolerance_smlib() {
    flips_keep_the_tolerance(kernel_smlib::SmlibKernel::new, "smlib");
}

#[cfg(feature = "solidcore")]
#[test]
fn flips_keep_the_tolerance_solidcore() {
    flips_keep_the_tolerance(kernel_solidcore::SolidcoreKernel::new, "solidcore");
}

/// Strongly twisted extrusions hold the tolerance, measured by sampling (`surface_distance`).
/// 20 turns of a 30 × 2 bar over 300 mm: its ruled sides are straight across but twisted
/// (`σuv · n`); the grid took that direction for straight (rows only, squashed in the CDT), so
/// Delaunay joined points across bands 7.5 mm wide whose chords the twist puts off the surface
/// and the refinement cycled ("tolerance 0.05 mm not reached", 0.32 mm after 40 passes); now the
/// twist counts as curvature in both directions. A circle of 10 turns scaled to 0.2, whose
/// chord at the rim measured 0.0529 at 0.05: the foot of a point beyond the end of the twisted
/// surface took the coupled Newton step's share in the free parameter (0.049).
fn twisted_extrusions<K: Kernel + 'static, F: Fn() -> K>(new: F, kernel: &str)
where
    K::Shape: 'static,
{
    let bar = "linear_extrude(height = 300, twist = 7200) square([30, 2], center = true);\n";
    let helix = "linear_extrude(height = 100, twist = 3600, scale = 0.2) circle(20);\n";
    for (name, src, max_tris) in [("bar", bar, 100_000), ("helix", helix, 200_000)] {
        with_part(new(), "spec/twist.mcad", Some(src), |k, s| {
            let n = holds(k, s, TOL, &format!("{kernel}: {name}"));
            assert!(n < max_tris, "{kernel}: {name}: {n} triangles");
            let brep = k.brep(s).unwrap();
            let m = cad_tess::tessellate(&brep, TOL).unwrap();
            let d = cad_tess::surface_distance(&brep, &m);
            assert!(d.chord.max <= TOL, "{kernel}: {name}: chord {} at {}", d.chord.max, d.chord.at);
        });
    }
}

#[test]
fn twisted_extrusions_smlib() {
    twisted_extrusions(kernel_smlib::SmlibKernel::new, "smlib");
}

#[cfg(feature = "solidcore")]
#[test]
fn twisted_extrusions_solidcore() {
    twisted_extrusions(kernel_solidcore::SolidcoreKernel::new, "solidcore");
}

/// The B-rep's edges are sampled by their chord sag for `edge_vertex_gap`, not by a fixed
/// count: the helical seam of a twisted extrusion (10 turns, 762 mm) against 512 segments was
/// 0.061 mm from the mesh's own vertices on it, a false gap at 0.05.
#[test]
fn long_free_form_edges_are_sampled_finely() {
    let src = "linear_extrude(height = 100, twist = 3600, scale = 0.2) circle(20);\n";
    with_part(kernel_smlib::SmlibKernel::new(), "spec/helix.mcad", Some(src), |k, s| {
        let brep = k.brep(s).unwrap();
        let m = cad_tess::tessellate(&brep, 0.05).unwrap();
        let d = cad_tess::surface_distance(&brep, &m);
        assert!(d.edge_vertex_gap <= 1e-3, "edge vertex gap {}", d.edge_vertex_gap);
    });
}
