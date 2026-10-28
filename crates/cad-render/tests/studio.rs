//! Studio look without a kernel: ground, shadows, ambient occlusion, light direction,
//! gradient background (meshes built by hand).

use cad_kernel::Mesh;
use cad_render::{CameraInfo, Image, Item, Projection, RenderOptions, Scene, Studio, View, render, render_info, to_u8};
use glam::DVec3;

/// Closed box from `lo` to `hi` (outward triangles).
fn boxm(lo: DVec3, hi: DVec3) -> Mesh {
    let mut m = Mesh::default();
    for i in 0..8u32 {
        let p = DVec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        m.positions.extend_from_slice(&[p.x as f32, p.y as f32, p.z as f32]);
    }
    for f in [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]] {
        m.indices.extend_from_slice(&[f[0], f[1], f[2], f[0], f[2], f[3]]);
    }
    m.tri_tags = vec![1; 12];
    m.tri_faces = (0..12).collect();
    m
}

fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

const GREY: [f32; 4] = [0.72, 0.73, 0.76, 1.0];

fn opts(view: View, studio: Option<Studio>) -> RenderOptions {
    RenderOptions { width: 240, height: 240, view, axes: false, studio, ..Default::default() }
}

/// Pixel of a scene point (orthographic camera).
fn px(c: &CameraInfo, o: &RenderOptions, p: DVec3) -> (u32, u32) {
    let d = p - c.target;
    let x = d.dot(c.right) * c.scale + o.width as f64 / 2.0;
    let y = o.height as f64 / 2.0 - d.dot(c.up) * c.scale;
    (x as u32, y as u32)
}

fn luma(img: &Image, (x, y): (u32, u32)) -> f64 {
    let p = img.pixel(x, y);
    (p[0] as f64 + p[1] as f64 + p[2] as f64) / 3.0
}

/// Mean brightness of a 5 × 5 window.
fn luma5(img: &Image, (x, y): (u32, u32)) -> f64 {
    let mut s = 0.0;
    for dy in 0..5 {
        for dx in 0..5 {
            s += luma(img, (x + dx - 2, y + dy - 2));
        }
    }
    s / 25.0
}

#[test]
fn studio_draws_ground_and_differs_from_standard() {
    let m = boxm(v(0.0, 0.0, 0.0), v(20.0, 20.0, 20.0));
    let scene = Scene { items: vec![Item::new(&m, GREY)], ..Default::default() };
    let std_img = render(&scene, &opts(View::Iso, None));
    let o = opts(View::Iso, Some(Studio::default()));
    let (img, cam) = render_info(&scene, &o);
    assert_ne!(img, std_img);
    let info = cam.studio.clone().expect("studio info");
    assert_eq!(info.ground_z, Some(0.0));
    assert_eq!(info.shadow_map, Some(2048));
    let r = info.ao_radius.unwrap();
    assert!((r - 20.0 * 3f64.sqrt() * 0.05).abs() < 1e-9, "{r}");
    // the ground: not the background colour near the model's base
    let bg = to_u8(o.background);
    let near = px(&cam, &o, v(30.0, 10.0, 0.0));
    assert_ne!(img.pixel(near.0, near.1), bg);
    // deterministic
    assert_eq!(render(&scene, &o), img);
    // ground off: a corner of the image is the background
    let no_ground = render(&scene, &opts(View::Iso, Some(Studio { ground: false, ..Studio::default() })));
    assert_eq!(no_ground.pixel(2, 2), bg);
}

#[test]
fn shadow_darkens_the_ground_behind_a_wall() {
    // wall at x 0..2; top view (camera azimuth 0): a light from the left (−x) at 40° throws
    // the shadow towards +x
    let wall = boxm(v(0.0, -20.0, 0.0), v(2.0, 20.0, 15.0));
    let foot = boxm(v(-30.0, -20.0, 0.0), v(-28.0, -18.0, 0.5));
    let far = boxm(v(40.0, 18.0, 0.0), v(42.0, 20.0, 0.5));
    let scene = Scene {
        items: vec![Item::new(&wall, GREY), Item::new(&foot, GREY), Item::new(&far, GREY)],
        ..Default::default()
    };
    let st =
        Studio { light_azimuth: -90.0, light_elevation: 40.0, ao: false, shadow_softness: 0.0, ..Studio::default() };
    let on = opts(View::Top, Some(st.clone()));
    let off = opts(View::Top, Some(Studio { shadows: false, ..st.clone() }));
    let (a, cam) = render_info(&scene, &on);
    let b = render(&scene, &off);
    let light = cam.studio.as_ref().unwrap().light_dir;
    assert!(light.x < -0.7 && light.z > 0.6, "{light}");
    // shadow length 15 / tan 40° ≈ 17.9 mm: x = 8 is in the shadow, x = −8 (lit side) is not
    let shadow = px(&cam, &on, v(8.0, 0.0, 0.0));
    let lit = px(&cam, &on, v(-8.0, 0.0, 0.0));
    let past = px(&cam, &on, v(30.0, 0.0, 0.0));
    assert!(luma5(&a, shadow) < luma5(&b, shadow) - 40.0, "{} vs {}", luma5(&a, shadow), luma5(&b, shadow));
    assert!((luma5(&a, lit) - luma5(&b, lit)).abs() < 2.0);
    assert!((luma5(&a, past) - luma5(&b, past)).abs() < 2.0);
    // softer shadows: the edge of the shadow (x ≈ 19.9) gets a wider transition
    let hard_edge: Vec<f64> = (16..24).map(|x| luma(&a, px(&cam, &on, v(x as f64, 0.0, 0.0)))).collect();
    let soft = render(&scene, &opts(View::Top, Some(Studio { shadow_softness: 1.0, ..st })));
    let soft_edge: Vec<f64> = (16..24).map(|x| luma(&soft, px(&cam, &on, v(x as f64, 0.0, 0.0)))).collect();
    let steps = |e: &[f64]| e.windows(2).filter(|w| (w[1] - w[0]).abs() > 3.0).count();
    assert!(steps(&soft_edge) > steps(&hard_edge), "hard {hard_edge:?} soft {soft_edge:?}");
}

#[test]
fn model_shadows_itself() {
    // a plate above a block: light from straight above shades the block's top
    let block = boxm(v(0.0, 0.0, 0.0), v(30.0, 30.0, 10.0));
    let plate = boxm(v(-5.0, -5.0, 20.0), v(15.0, 35.0, 22.0));
    let scene = Scene { items: vec![Item::new(&block, GREY), Item::new(&plate, GREY)], ..Default::default() };
    let st = Studio { light_azimuth: 0.0, light_elevation: 90.0, ao: false, ..Studio::default() };
    let o = opts(View::Iso, Some(st.clone()));
    let (a, cam) = render_info(&scene, &o);
    let b = render(&scene, &opts(View::Iso, Some(Studio { shadows: false, ..st })));
    // top of the block: under the plate (x = 12) in shadow, beside it (x = 26) lit
    let under = px(&cam, &o, v(12.0, 2.0, 10.0));
    let beside = px(&cam, &o, v(26.0, 2.0, 10.0));
    assert!(luma5(&a, under) < luma5(&b, under) - 40.0);
    assert!((luma5(&a, beside) - luma5(&b, beside)).abs() < 2.0);
}

#[test]
fn ambient_occlusion_darkens_a_concave_corner() {
    // L profile: floor plate and a wall at its back; iso view looks into the corner
    let floor = boxm(v(0.0, 0.0, 0.0), v(40.0, 40.0, 4.0));
    let wall = boxm(v(0.0, 36.0, 4.0), v(40.0, 40.0, 30.0));
    let scene = Scene { items: vec![Item::new(&floor, GREY), Item::new(&wall, GREY)], ..Default::default() };
    let st = Studio { shadows: false, ao_radius: Some(5.0), ..Studio::default() };
    let o = opts(View::Iso, Some(st.clone()));
    let (a, cam) = render_info(&scene, &o);
    assert_eq!(cam.studio.as_ref().unwrap().ao_radius, Some(5.0));
    let b = render(&scene, &opts(View::Iso, Some(Studio { ao: false, ..st.clone() })));
    let corner = px(&cam, &o, v(20.0, 35.0, 4.0));
    let open = px(&cam, &o, v(20.0, 8.0, 4.0));
    assert!(luma5(&a, corner) < luma5(&b, corner) - 8.0, "{} vs {}", luma5(&a, corner), luma5(&b, corner));
    assert!((luma5(&a, open) - luma5(&b, open)).abs() < 3.0, "{} vs {}", luma5(&a, open), luma5(&b, open));
    // strength 2 darkens more than strength 0.5
    let strong = render(&scene, &opts(View::Iso, Some(Studio { ao_strength: 2.0, ..st.clone() })));
    let weak = render(&scene, &opts(View::Iso, Some(Studio { ao_strength: 0.5, ..st })));
    assert!(luma5(&strong, corner) < luma5(&weak, corner) - 4.0);
}

#[test]
fn ground_only_from_above_and_lights_follow_the_camera() {
    let m = boxm(v(0.0, 0.0, 0.0), v(10.0, 10.0, 10.0));
    let scene = Scene { items: vec![Item::new(&m, GREY)], ..Default::default() };
    let info = |view: View, st: Studio| render_info(&scene, &opts(view, Some(st))).1.studio.unwrap();
    assert_eq!(info(View::Bottom, Studio::default()).ground_z, None);
    assert_eq!(info(View::Front, Studio::default()).ground_z, None);
    assert_eq!(info(View::Iso, Studio::default()).ground_z, Some(0.0));
    // azimuth relative to the camera: 0 = from the camera, 90 = from its right
    let s = |az: f64, el: f64| Studio { light_azimuth: az, light_elevation: el, ..Studio::default() };
    let d = info(View::Front, s(0.0, 0.0)).light_dir;
    assert!((d - DVec3::NEG_Y).length() < 1e-3, "{d}");
    let d = info(View::Front, s(90.0, 0.0)).light_dir;
    assert!((d - DVec3::X).length() < 1e-3, "{d}");
    let d = info(View::Right, s(0.0, 0.0)).light_dir;
    assert!((d - DVec3::X).length() < 1e-3, "{d}");
    let d = info(View::Top, s(0.0, 45.0)).light_dir;
    assert!((d - DVec3::new(0.0, -1.0, 1.0).normalize()).length() < 1e-3, "{d}");
    // perspective and every standard view render
    for name in ["iso", "front", "back", "top", "bottom", "left", "right"] {
        let o = RenderOptions {
            projection: Projection::Perspective { fov_deg: 35.0 },
            ..opts(View::from_name(name).unwrap(), Some(Studio::default()))
        };
        let img = render(&scene, &o);
        assert!(img.count_not(to_u8(o.background)) > 1000, "{name}");
    }
}

#[test]
fn gradient_background() {
    let o = RenderOptions {
        width: 64,
        height: 64,
        axes: false,
        background: [1.0, 1.0, 1.0, 1.0],
        background_bottom: Some([0.0, 0.0, 0.0, 1.0]),
        supersample: 1,
        ..Default::default()
    };
    let img = render(&Scene::default(), &o);
    assert_eq!(img.pixel(10, 0), [255, 255, 255, 255]);
    assert_eq!(img.pixel(10, 63), [0, 0, 0, 255]);
    let mid = img.pixel(10, 32)[0];
    assert!((120..=135).contains(&mid), "{mid}");
}

#[test]
fn transparent_items_cast_no_shadow() {
    let m = boxm(v(0.0, -20.0, 0.0), v(2.0, 20.0, 15.0));
    let base = boxm(v(-30.0, -20.0, 0.0), v(-28.0, -18.0, 0.5));
    let glass = [0.5, 0.6, 0.9, 0.3];
    let scene = Scene { items: vec![Item::new(&m, glass), Item::new(&base, GREY)], ..Default::default() };
    let st = Studio { light_azimuth: -90.0, light_elevation: 40.0, ao: false, ..Studio::default() };
    let o = opts(View::Top, Some(st.clone()));
    let (a, cam) = render_info(&scene, &o);
    let b = render(&scene, &opts(View::Top, Some(Studio { shadows: false, ..st })));
    let p = px(&cam, &o, v(8.0, 0.0, 0.0));
    assert!((luma5(&a, p) - luma5(&b, p)).abs() < 2.0);
}

/// The 12 edges of a box as segments (`Mesh::edge_segments`).
fn box_edges(lo: DVec3, hi: DVec3) -> Vec<f32> {
    let c = |i: u32| {
        [
            if i & 1 == 0 { lo.x } else { hi.x } as f32,
            if i & 2 == 0 { lo.y } else { hi.y } as f32,
            if i & 4 == 0 { lo.z } else { hi.z } as f32,
        ]
    };
    let mut out = Vec::new();
    for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
        out.extend_from_slice(&c(a));
        out.extend_from_slice(&c(b));
    }
    out
}

#[test]
fn edges_off_hides_the_edge_lines() {
    let (lo, hi) = (v(0.0, 0.0, 0.0), v(20.0, 20.0, 20.0));
    let mut m = boxm(lo, hi);
    m.edge_segments = box_edges(lo, hi);
    let black = Some([0.0, 0.0, 0.0, 1.0]);
    let scene = Scene { items: vec![Item::new(&m, GREY).with_edges(black)], ..Default::default() };
    let st = Studio { shadows: false, ao: false, ground: false, ..Studio::default() };
    let o = opts(View::Iso, Some(st.clone()));
    let (on, cam) = render_info(&scene, &o);
    let off = render(&scene, &opts(View::Iso, Some(Studio { edges: false, ..st })));
    assert_ne!(on, off);
    // the vertical front edge (x = 20, y = 0) at mid height: darker with edges
    let p = px(&cam, &o, v(20.0, 0.0, 10.0));
    let (a, b) = (luma(&on, p).min(luma(&on, (p.0 + 1, p.1))), luma(&off, p).min(luma(&off, (p.0 + 1, p.1))));
    assert!(a + 15.0 < b, "edge {a}, without {b}");
    // the faces away from the edges are the same
    let f = px(&cam, &o, v(10.0, 0.0, 10.0));
    assert!((luma5(&on, f) - luma5(&off, f)).abs() < 1.0);
}
