use glam::DVec3;

use super::*;

pub(crate) fn cube_tris(s: f32) -> Vec<[[f32; 3]; 3]> {
    let p = |i: u32| [(i & 1) as f32 * s, ((i >> 1) & 1) as f32 * s, ((i >> 2) & 1) as f32 * s];
    let quads: [[u32; 4]; 6] = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    quads.iter().flat_map(|q| [[p(q[0]), p(q[1]), p(q[2])], [p(q[0]), p(q[2]), p(q[3])]]).collect()
}

fn d(t: &[[[f32; 3]; 3]]) -> Vec<[DVec3; 3]> {
    t.iter().map(|t| t.map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64))).collect()
}

#[test]
fn ascii_and_binary() {
    let c = cube_tris(10.0);
    let a = parse(write_ascii("wuerfel", &c).as_bytes()).unwrap();
    assert!(!a.binary);
    assert_eq!(a.name.as_deref(), Some("wuerfel"));
    assert_eq!(a.triangles, c);
    let b = parse(&write_binary("binär", &c)).unwrap();
    assert!(b.binary);
    assert_eq!(b.triangles, c);
}

#[test]
fn binary_starting_with_solid() {
    // header "solid …" as many exporters write it: the size decides
    let c = cube_tris(1.0);
    let b = parse(&write_binary("solid exported by some CAD", &c)).unwrap();
    assert!(b.binary);
    assert_eq!(b.triangles.len(), 12);
}

#[test]
fn broken_files() {
    assert!(parse(b"").unwrap_err().contains("leer"));
    assert!(parse(b"hello").unwrap_err().contains("keine STL"));
    let mut bin = write_binary("x", &cube_tris(1.0));
    bin.truncate(bin.len() - 7);
    assert!(parse(&bin).unwrap_err().contains("abgeschnitten"), "{:?}", parse(&bin));
    let e =
        parse(b"solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nendloop\nendfacet\nendsolid\n")
            .unwrap_err();
    assert!(e.contains("2 statt 3"), "{e}");
    let e = parse(b"solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 a\n").unwrap_err();
    assert!(e.contains("Zeile 4") && e.contains("keine Zahl"), "{e}");
    let e = parse(b"solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\n")
        .unwrap_err();
    assert!(e.contains("abgeschnitten"), "{e}");
    // empty solid: parsed, 0 triangles (prepare reports it)
    assert!(parse(b"solid leer\nendsolid leer\n").unwrap().triangles.is_empty());
    assert!(prepare(&[]).unwrap_err().contains("0 Dreiecke"));
}

#[test]
fn cube_merges_to_six_faces() {
    let p = prepare(&d(&cube_tris(10.0))).unwrap();
    assert_eq!(p.stats.faces, 6);
    assert_eq!(p.mesh.loops.iter().map(|l| l.len()).collect::<Vec<_>>(), vec![4; 6]);
    assert!(p.mesh.closed);
    assert_eq!(p.stats.flipped, 0);
    assert_eq!(p.stats.vertices, 8);
    let mp = p.mesh.mass_props(&glam::DMat4::IDENTITY);
    assert!((mp.volume - 1000.0).abs() < 1e-9);
}

#[test]
fn inverted_and_mixed_orientation() {
    let mut c = cube_tris(2.0);
    for t in &mut c {
        t.swap(1, 2);
    }
    let p = prepare(&d(&c)).unwrap();
    assert_eq!(p.stats.flipped, 12);
    assert_eq!(p.stats.inverted_parts, 1);
    assert!((p.mesh.mass_props(&glam::DMat4::IDENTITY).volume - 8.0).abs() < 1e-9);
    let mut c = cube_tris(2.0);
    c[3].swap(1, 2);
    c[7].swap(1, 2);
    let p = prepare(&d(&c)).unwrap();
    assert_eq!(p.stats.flipped, 2);
    assert_eq!(p.stats.inverted_parts, 0);
    assert!((p.mesh.mass_props(&glam::DMat4::IDENTITY).volume - 8.0).abs() < 1e-9);
}

#[test]
fn open_nan_degenerate_weld() {
    let mut c = cube_tris(1.0);
    c.pop(); // one triangle missing: open
    let mut t = d(&c);
    t.push([DVec3::new(f64::NAN, 0.0, 0.0), DVec3::ZERO, DVec3::X]);
    t.push([DVec3::ZERO, DVec3::ZERO, DVec3::X]);
    t.push(t[0]);
    // a corner off by less than the weld tolerance
    t[1][0].x += 1e-8;
    let p = prepare(&t).unwrap();
    assert_eq!(p.stats.non_finite, 1);
    assert_eq!(p.stats.degenerate, 1);
    assert_eq!(p.stats.duplicate, 1);
    assert_eq!(p.stats.open_edges, 3);
    assert_eq!(p.stats.vertices, 8);
    assert!(!p.mesh.closed);
}

#[test]
fn voids_point_inward() {
    // a cube with a cubic void: two closed parts, the inner one inward
    let mut t = d(&cube_tris(10.0));
    let mut inner = d(&cube_tris(2.0));
    for tri in &mut inner {
        for p in tri.iter_mut() {
            *p += DVec3::splat(4.0);
        }
    }
    t.extend(inner);
    let p = prepare(&t).unwrap();
    assert_eq!(p.stats.components, 2);
    assert_eq!(p.stats.faces, 12);
    let v = p.mesh.mass_props(&glam::DMat4::IDENTITY).volume;
    assert!((v - 992.0).abs() < 1e-9, "{v}");
    let b = p.mesh.brep(&glam::DMat4::IDENTITY, 1);
    b.validate().unwrap();
    assert!(matches!(&b.bodies[..], [cad_kernel::brep::Body::Solid { voids, .. }] if voids.len() == 1));
}

#[test]
fn collinear_vertices_dropped_from_loops() {
    // a 2 × 1 × 1 box whose long faces are split in the middle (vertices on the edges)
    let mut pts = Vec::new();
    for z in [0.0, 1.0] {
        for y in [0.0, 1.0] {
            for x in [0.0, 1.0, 2.0] {
                pts.push(DVec3::new(x, y, z));
            }
        }
    }
    let i = |x: usize, y: usize, z: usize| pts[x + 3 * y + 6 * z];
    let mut t: Vec<[DVec3; 3]> = Vec::new();
    let mut quad = |a: DVec3, b: DVec3, c: DVec3, e: DVec3| {
        t.push([a, b, c]);
        t.push([a, c, e]);
    };
    for x in 0..2 {
        quad(i(x, 0, 0), i(x, 1, 0), i(x + 1, 1, 0), i(x + 1, 0, 0)); // bottom (−z)
        quad(i(x, 0, 1), i(x + 1, 0, 1), i(x + 1, 1, 1), i(x, 1, 1)); // top
        quad(i(x, 0, 0), i(x + 1, 0, 0), i(x + 1, 0, 1), i(x, 0, 1)); // front (−y)
        quad(i(x, 1, 0), i(x, 1, 1), i(x + 1, 1, 1), i(x + 1, 1, 0)); // back
    }
    quad(i(0, 0, 0), i(0, 0, 1), i(0, 1, 1), i(0, 1, 0)); // −x
    quad(i(2, 0, 0), i(2, 1, 0), i(2, 1, 1), i(2, 0, 1)); // +x
    let p = prepare(&t).unwrap();
    assert_eq!(p.stats.faces, 6);
    assert!(p.mesh.loops.iter().all(|l| l.len() == 4), "{:?}", p.mesh.loops);
    p.mesh.brep(&glam::DMat4::IDENTITY, 1).validate().unwrap();
    assert_eq!(p.mesh.brep(&glam::DMat4::IDENTITY, 1).edges.len(), 12);
}

#[test]
fn rotated_cube_is_snapped_planar() {
    // f32 coordinates of a rotated cube: the quads are planar only up to rounding
    let (a, b) = (30f64.to_radians(), 20f64.to_radians());
    let rot = |p: [f32; 3]| {
        let (x, y, z) = (p[0] as f64, p[1] as f64, p[2] as f64);
        let (x, y) = (x * a.cos() - y * a.sin(), x * a.sin() + y * a.cos());
        let (y, z) = (y * b.cos() - z * b.sin(), y * b.sin() + z * b.cos());
        [x as f32, y as f32, z as f32]
    };
    let c: Vec<[[f32; 3]; 3]> = cube_tris(10.0).iter().map(|t| t.map(rot)).collect();
    let p = prepare(&d(&c)).unwrap();
    assert_eq!(p.stats.faces, 6);
    let m = &p.mesh;
    for (t, &f) in m.tris.iter().zip(&m.tri_face) {
        let l = &m.loops[f as usize];
        let n = (m.points[l[1] as usize] - m.points[l[0] as usize])
            .cross(m.points[l[2] as usize] - m.points[l[0] as usize])
            .normalize();
        for &v in t {
            assert!((n.dot(m.points[v as usize] - m.points[l[0] as usize])).abs() < 1e-11, "face {f}");
        }
    }
    assert!((m.mass_props(&glam::DMat4::IDENTITY).volume - 1000.0).abs() < 1e-3);
}

#[test]
fn plane_with_hole_becomes_few_faces() {
    // a 10 × 10 × 1 plate with a 4 × 4 square hole: each ring face splits into two faces
    // without holes (not into its 8 triangles)
    let sq = |h: f64, z: f64| -> Vec<DVec3> {
        vec![DVec3::new(-h, -h, z), DVec3::new(h, -h, z), DVec3::new(h, h, z), DVec3::new(-h, h, z)]
    };
    let (o0, o1, i0, i1) = (sq(5.0, 0.0), sq(5.0, 1.0), sq(2.0, 0.0), sq(2.0, 1.0));
    let mut t: Vec<[DVec3; 3]> = Vec::new();
    for k in 0..4 {
        let j = (k + 1) % 4;
        // top ring (+z), bottom ring (−z)
        t.push([o1[k], o1[j], i1[j]]);
        t.push([o1[k], i1[j], i1[k]]);
        t.push([o0[k], i0[j], o0[j]]);
        t.push([o0[k], i0[k], i0[j]]);
        // outer and inner walls
        t.push([o0[k], o0[j], o1[j]]);
        t.push([o0[k], o1[j], o1[k]]);
        t.push([i0[k], i1[j], i0[j]]);
        t.push([i0[k], i1[k], i1[j]]);
    }
    let p = prepare(&t).unwrap();
    assert!(p.mesh.closed);
    assert_eq!(p.stats.faces, 12, "{:?}", p.mesh.loops);
    assert!((p.mesh.mass_props(&glam::DMat4::IDENTITY).volume - 84.0).abs() < 1e-9);
    p.mesh.brep(&glam::DMat4::IDENTITY, 1).validate().unwrap();
}
