//! Renders engine results (native SMLib) and writes example images to `tests/out/`.

use std::path::PathBuf;

use cad_engine::Engine;
use cad_engine::builder::*;
use cad_ir::Select;
use cad_render::{Item, Polyline, RenderOptions, Scene, View, render, to_u8};
use glam::DVec3;
use kernel_smlib::SmlibKernel;

fn out_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/out");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

const PART: [f32; 4] = [0.75, 0.78, 0.84, 1.0];
const RED: [f32; 4] = [0.9, 0.2, 0.15, 0.35];
const BLUE: [f32; 4] = [0.2, 0.45, 0.95, 0.3];

#[test]
fn renders_difference_with_subtracted_body() {
    let mut e = Engine::new(SmlibKernel::new());
    let mut b = TreeBuilder::new();
    let d = b.root(difference_fillet(1.0));
    b.child(d, cube(20.0));
    let t = b.child(d, translate(v(10.0, 10.0, -5.0)));
    let cyl = b.child(t, cylinder(30.0, 5.0));
    let ev = e.evaluate(b.finish());
    let root = e.tessellate(&ev, d, None).unwrap();
    let cm = e.tessellate(&ev, cyl, Some(0.1)).unwrap();
    let scene = Scene {
        items: vec![
            Item::new(&root, PART),
            Item::new(&cm, RED).with_transform(ev.report(cyl).display_world).with_edges(None),
        ],
        lines: ev
            .report(d)
            .selected_edges
            .iter()
            .map(|p| Polyline { points: p.clone(), color: [1.0, 0.55, 0.0, 1.0], width: 2.5, on_top: false })
            .collect(),
    };
    let opts = RenderOptions { width: 640, height: 480, ..Default::default() };
    let img = render(&scene, &opts);
    assert_eq!((img.width, img.height), (640, 480));
    let bg = to_u8(opts.background);
    let drawn = img.count_not(bg);
    assert!(drawn > 640 * 480 / 10, "{drawn} pixels drawn");
    // centre pixel shows the part, a corner shows the background
    assert_ne!(img.pixel(320, 240), bg);
    assert_eq!(img.pixel(639, 0), bg);
    let png = img.to_png();
    assert_eq!(&png[1..4], b"PNG");
    // decodes again
    let dec = png::Decoder::new(std::io::Cursor::new(&png));
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (640, 480));
    assert_eq!(&buf[..info.buffer_size()], &img.rgba[..]);
    std::fs::write(out_dir().join("difference_iso.png"), &png).unwrap();

    // front view
    let img = render(&scene, &RenderOptions { view: View::Front, ..opts.clone() });
    assert!(img.count_not(bg) > 1000);
    img.write_png(out_dir().join("difference_front.png")).unwrap();
}

#[test]
fn renders_zone_block_and_profile() {
    let mut e = Engine::new(SmlibKernel::new());
    let mut b = TreeBuilder::new();
    let f = b.root(fillet(2.0, Select::Inside));
    b.child(f, cube(20.0));
    let z = b.child(f, zone_line(v(20.0, 0.0, 20.0), DVec3::Y, 3.0));
    b.flags(z).zone = true;
    let ev = e.evaluate(b.finish());
    let m = e.tessellate(&ev, f, None).unwrap();
    let zm = e.tessellate(&ev, z, None).unwrap();
    let scene = Scene { items: vec![Item::new(&m, PART), Item::new(&zm, BLUE).with_edges(None)], lines: vec![] };
    let img = render(&scene, &RenderOptions::default());
    assert!(img.count_not(to_u8(RenderOptions::default().background)) > 10_000);
    img.write_png(out_dir().join("zone_block_iso.png")).unwrap();

    // 2D profile (square minus circle) from the top
    let mut b = TreeBuilder::new();
    let d = b.root(difference());
    b.child(d, square(20.0));
    let t = b.child(d, translate(v(10.0, 10.0, 0.0)));
    b.child(t, circle(5.0));
    let ev = e.evaluate(b.finish());
    let m = e.tessellate(&ev, d, None).unwrap();
    let opts = RenderOptions { view: View::Top, width: 400, height: 400, axes: false, ..Default::default() };
    let img = render(&Scene { items: vec![Item::new(&m, [0.55, 0.7, 0.9, 1.0])], lines: vec![] }, &opts);
    let bg = to_u8(opts.background);
    // the hole in the middle shows the background
    assert_eq!(img.pixel(200, 200), bg);
    assert_ne!(img.pixel(80, 80), bg);
    img.write_png(out_dir().join("profile_top.png")).unwrap();
}

/// Square [0, 10]² at height z, two triangles split along the diagonal (0,0)-(10,10) or, with
/// `other_diagonal`, along (10,0)-(0,10).
fn square_mesh(z: f32, other_diagonal: bool) -> cad_kernel::Mesh {
    let mut m = cad_kernel::Mesh::default();
    for (x, y) in [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)] {
        m.positions.extend_from_slice(&[x, y, z]);
        m.normals.extend_from_slice(&[0.0, 0.0, 1.0]);
    }
    m.indices = if other_diagonal { vec![0, 1, 3, 1, 2, 3] } else { vec![0, 1, 2, 0, 2, 3] };
    m.tri_tags = vec![1, 1];
    m.tri_faces = vec![0, 0];
    m
}

/// Coplanar transparent faces of two meshes (a subtracted body over the cavity it leaves) and
/// stacked transparent layers must blend to one uniform colour per region. Sorting whole
/// triangles by centroid depth flipped the order along the triangle diagonals (diagonal bands
/// in `cad render --line` of a subtracted box).
#[test]
fn transparent_coplanar_faces_have_no_diagonal_bands() {
    let a = square_mesh(0.0, false);
    let b = square_mesh(0.0, true);
    let items = vec![Item::new(&a, [0.78, 0.79, 0.82, 0.22]).with_edges(None), Item::new(&b, RED).with_edges(None)];
    // iso: the plane is oblique, so the triangle centroids lie at different depths
    let opts = RenderOptions { width: 240, height: 180, axes: false, ..Default::default() };
    let img = render(&Scene { items, lines: vec![] }, &opts);
    let bg = to_u8(opts.background);
    let inside = |x: u32, y: u32| {
        (-3i32..=3).all(|dy| (-3i32..=3).all(|dx| img.pixel((x as i32 + dx) as u32, (y as i32 + dy) as u32) != bg))
    };
    let mut colours = std::collections::BTreeSet::new();
    let mut n = 0;
    for y in 4..176 {
        for x in 4..236 {
            if inside(x, y) {
                colours.insert(img.pixel(x, y));
                n += 1;
            }
        }
    }
    assert!(n > 2000, "{n} interior pixels");
    assert_eq!(colours.len(), 1, "coplanar transparent faces blend to {colours:?}");
    // the red square is the later item: it ends on top of the coincident grey one
    let c = *colours.iter().next().unwrap();
    assert!(c[0] as i32 > c[2] as i32 + 20, "{c:?}");
}

#[test]
fn empty_scene_is_background() {
    let opts = RenderOptions { width: 32, height: 16, axes: false, ..Default::default() };
    let img = render(&Scene::default(), &opts);
    assert_eq!(img.count_not(to_u8(opts.background)), 0);
}
