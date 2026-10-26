//! Renderer features without a kernel: camera angles, section caps, perspective, markers,
//! labels, contact sheets (meshes built by hand).

use cad_kernel::Mesh;
use cad_render::{
    Item, Label, Marker, MarkerShape, Projection, RenderOptions, Scene, Section, View, contact_sheet, fit_scale,
    render, render_info, to_u8,
};
use glam::DVec3;

/// Closed unit cube [0, s]³ (outward triangles, no shared normals needed).
fn cube(o: DVec3, s: f64) -> Mesh {
    let mut m = Mesh::default();
    let c = |i: u32| o + DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64) * s;
    for i in 0..8 {
        let p = c(i);
        m.positions.extend_from_slice(&[p.x as f32, p.y as f32, p.z as f32]);
    }
    for f in [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]] {
        m.indices.extend_from_slice(&[f[0], f[1], f[2], f[0], f[2], f[3]]);
    }
    m.tri_tags = vec![1; 12];
    m.tri_faces = (0..12).collect();
    m
}

const GREY: [f32; 4] = [0.7, 0.7, 0.72, 1.0];

fn opts(view: View) -> RenderOptions {
    RenderOptions { width: 200, height: 200, view, axes: false, ..Default::default() }
}

fn count(img: &cad_render::Image, f: impl Fn([u8; 4]) -> bool) -> usize {
    (0..img.height).flat_map(|y| (0..img.width).map(move |x| (x, y))).filter(|&(x, y)| f(img.pixel(x, y))).count()
}

fn is_cap(p: [u8; 4]) -> bool {
    p[0] as i32 > p[1] as i32 + 60 && p[0] as i32 > p[2] as i32 + 60
}

#[test]
fn angles_match_named_views() {
    for (name, az, el) in [("iso", 45.0, 35.26438968), ("front", 0.0, 0.0), ("right", 90.0, 0.0), ("top", 0.0, 90.0)] {
        let v = View::from_name(name).unwrap();
        let (a, e) = v.angles();
        assert!((a - az).abs() < 1e-6 && (e - el).abs() < 1e-6, "{name}: {a} {e}");
        let (d0, _) = v.basis();
        let (d1, _) = View::from_angles(az, el).basis();
        assert!((d0.normalize() - d1.normalize()).length() < 1e-9, "{name}");
    }
}

#[test]
fn section_clips_and_caps_a_closed_mesh() {
    let m = cube(DVec3::ZERO, 10.0);
    let scene = Scene { items: vec![Item::new(&m, GREY).with_edges(None)], ..Default::default() };
    let full = render(&scene, &opts(View::Top));
    assert_eq!(count(&full, is_cap), 0);
    // cut at z = 5, seen from above: the whole square is cap
    let sec = Section { point: DVec3::new(0.0, 0.0, 5.0), normal: DVec3::Z };
    let o = RenderOptions { section: Some(sec), ..opts(View::Top) };
    let cut = render(&scene, &o);
    let bg = to_u8(o.background);
    let drawn = cut.count_not(bg);
    let caps = count(&cut, is_cap);
    assert!(caps as f64 > drawn as f64 * 0.95, "{caps} of {drawn}");
    // iso: the cut reduces the silhouette, the cap is visible
    let iso_full = render(&scene, &opts(View::Iso)).count_not(bg);
    let iso_cut = render(&scene, &RenderOptions { section: Some(sec), ..opts(View::Iso) });
    assert!(iso_cut.count_not(bg) < iso_full);
    assert!(count(&iso_cut, is_cap) > 1000);
    // seen from below the kept half hides the cap
    let below = render(&scene, &RenderOptions { section: Some(sec), ..opts(View::Bottom) });
    assert_eq!(count(&below, is_cap), 0);
    // a plane outside removes nothing / everything
    let none = render(
        &scene,
        &RenderOptions { section: Some(Section { point: DVec3::splat(20.0), normal: DVec3::Z }), ..opts(View::Iso) },
    );
    assert_eq!(none.count_not(bg), iso_full);
    let all = render(
        &scene,
        &RenderOptions { section: Some(Section { point: DVec3::splat(-1.0), normal: DVec3::Z }), ..opts(View::Iso) },
    );
    assert_eq!(all.count_not(bg), 0);
    // no cap for items that opt out
    let scene2 = Scene { items: vec![Item::new(&m, GREY).with_edges(None).with_cap(false)], ..Default::default() };
    assert_eq!(count(&render(&scene2, &o), is_cap), 0);
}

#[test]
fn perspective_and_fixed_scale() {
    let near = cube(DVec3::new(0.0, -20.0, 0.0), 10.0);
    let far = cube(DVec3::new(0.0, 20.0, 0.0), 10.0);
    let scene = Scene {
        items: vec![
            Item::new(&near, [0.9, 0.1, 0.1, 1.0]).with_edges(None),
            Item::new(&far, [0.1, 0.1, 0.9, 1.0]).with_edges(None),
        ],
        ..Default::default()
    };
    let o = RenderOptions { projection: Projection::Perspective { fov_deg: 50.0 }, ..opts(View::Front) };
    let (img, cam) = render_info(&scene, &o);
    assert!(cam.eye.y < -20.0, "{:?}", cam.eye);
    // the near cube hides the far one in front view... seen slightly from the side both show,
    // the near one larger
    let o = RenderOptions { view: View::from_angles(25.0, 10.0), ..o };
    let img2 = render(&scene, &o);
    let red = count(&img2, |p| p[0] > 120 && p[2] < 80);
    let blue = count(&img2, |p| p[2] > 120 && p[0] < 80);
    assert!(red > blue && blue > 0, "{red} {blue}");
    assert_ne!(img, img2);
    // fixed scale: pixels per mm as given
    let s = fit_scale(&scene, &opts(View::Front));
    let (_, c2) = render_info(&scene, &RenderOptions { scale: Some(s / 2.0), ..opts(View::Front) });
    assert!((c2.scale - s / 2.0).abs() < 1e-9);
    assert!((c2.width_mm - 200.0 / (s / 2.0)).abs() < 1e-9);
}

#[test]
fn markers_labels_title_and_sheet() {
    let m = cube(DVec3::ZERO, 10.0);
    let base = Scene { items: vec![Item::new(&m, GREY)], ..Default::default() };
    let plain = render(&base, &opts(View::Iso));
    let mut s = base.clone();
    s.markers.push(Marker {
        pos: DVec3::new(10.0, 10.0, 10.0),
        shape: MarkerShape::Diamond,
        size: 6.0,
        color: [1.0, 0.8, 0.0, 1.0],
        on_top: true,
    });
    s.labels.push(Label {
        pos: DVec3::ZERO,
        text: "g[left,front,floor]".into(),
        color: [0.0, 0.0, 0.0, 1.0],
        offset: (4.0, 4.0),
        priority: 1,
    });
    // an overlapping label of lower priority is moved or dropped, never drawn on top
    s.labels.push(Label {
        pos: DVec3::ZERO,
        text: "zweites".into(),
        color: [0.0, 0.0, 0.0, 1.0],
        offset: (4.0, 4.0),
        priority: 0,
    });
    let img = render(&s, &RenderOptions { title: Some("iso".into()), ..opts(View::Iso) });
    let diff = (0..200)
        .flat_map(|y| (0..200).map(move |x| (x, y)))
        .filter(|&(x, y)| img.pixel(x, y) != plain.pixel(x, y))
        .count();
    assert!(diff > 300, "{diff}");
    let sheet = contact_sheet(&[img.clone(), plain.clone(), img.clone()], 2, 8, [0.96, 0.96, 0.97, 1.0]);
    assert_eq!((sheet.width, sheet.height), (2 * 200 + 3 * 8, 2 * 200 + 3 * 8));
    assert_eq!(sheet.pixel(8 + 100, 8 + 100), img.pixel(100, 100));
    assert_eq!(sheet.pixel(16 + 200 + 50, 8 + 50), plain.pixel(50, 50));
}
