//! Software rasterizer for the CLI (`render`): meshes + camera → RGBA image → PNG.
//!
//! * Camera: orthographic (default) or perspective, standard views or azimuth/elevation,
//!   fitted to the scene or to given bounds, optional look-at target, zoom and fixed scale
//!   (contact sheets with equal scale per tile).
//! * Z-buffer, per-vertex Lambert shading (two-sided), edge lines from [`Mesh::edge_segments`]
//!   with depth test, colour per mesh, transparency (transparent fragments are sorted back to
//!   front per sample and blended over the opaque image), extra polylines (solid or dashed),
//!   point markers (diamonds, dots, squares), text labels with overlap avoidance, a title,
//!   optional axis triad.
//! * Section ([`Section`]): meshes are clipped against a plane in the renderer and the cut is
//!   closed by a filled cap in its own colour — the same principle as GPU clipping with a
//!   stencil cap. The cap covers every sample whose ray crosses the item's surface an odd
//!   number of times before it reaches the plane (the plane point lies inside the solid),
//!   counted with a top-left fill rule so shared edges count once. Convention: the plane
//!   through `point` with `normal` removes the half space the normal points into, i.e. what
//!   stays is `(q − point) · normal ≤ 0`.
//! * Studio look ([`Studio`], `RenderOptions::studio`): the opaque samples are shaded after
//!   rasterisation (deferred) with a key light (Blinn-Phong highlight), a camera fill light and
//!   hemisphere ambient light, soft shadows from a shadow map (percentage-closer soft shadows),
//!   screen-space ambient occlusion and a ground plane under the model that fades into the
//!   background; see [`studio`].
//! * Background: solid or a vertical gradient (`RenderOptions::background_bottom`).
//! * Anti-aliasing by supersampling. No GPU.

mod font;
pub mod studio;
pub mod text;

use cad_kernel::Mesh;
use glam::{DMat4, DVec3};

pub use studio::{Studio, StudioInfo};
pub use text::{TextSize, contact_sheet, text_size, tile_origin};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum View {
    /// From (+x, −y, +z), z up.
    Iso,
    /// From −y, z up.
    Front,
    /// From +y, z up.
    Back,
    /// From +z, y up.
    Top,
    /// From −z, y up.
    Bottom,
    /// From −x, z up.
    Left,
    /// From +x, z up.
    Right,
    /// `dir`: viewing direction (camera → scene).
    Custom { dir: DVec3, up: DVec3 },
}

impl View {
    pub fn from_name(s: &str) -> Option<View> {
        Some(match s {
            "iso" => View::Iso,
            "front" => View::Front,
            "back" => View::Back,
            "top" => View::Top,
            "bottom" => View::Bottom,
            "left" => View::Left,
            "right" => View::Right,
            _ => return None,
        })
    }

    /// Camera on a sphere around the target: azimuth 0° = from −y (front), 90° = from +x
    /// (right), counter-clockwise seen from above; elevation 90° = from above. `iso` is
    /// azimuth 45°, elevation 35.26°.
    pub fn from_angles(azimuth_deg: f64, elevation_deg: f64) -> View {
        let (a, e) = (azimuth_deg.to_radians(), elevation_deg.clamp(-90.0, 90.0).to_radians());
        let eye = DVec3::new(a.sin() * e.cos(), -a.cos() * e.cos(), e.sin());
        let up = if e.cos() < 1e-6 {
            // straight from above or below: y of the azimuth's front is up
            DVec3::new(-a.sin(), a.cos(), 0.0) * e.sin().signum()
        } else {
            DVec3::Z
        };
        View::Custom { dir: -eye, up }
    }

    /// (azimuth, elevation) in degrees of the viewing direction (see [`View::from_angles`]).
    pub fn angles(&self) -> (f64, f64) {
        let eye = -self.basis().0.normalize();
        let el = eye.z.clamp(-1.0, 1.0).asin().to_degrees();
        let az = if eye.x.abs() < 1e-12 && eye.y.abs() < 1e-12 { 0.0 } else { eye.x.atan2(-eye.y).to_degrees() };
        (az, el)
    }

    /// (viewing direction, up)
    pub fn basis(&self) -> (DVec3, DVec3) {
        match *self {
            View::Iso => (DVec3::new(-1.0, 1.0, -1.0), DVec3::Z),
            View::Front => (DVec3::Y, DVec3::Z),
            View::Back => (DVec3::NEG_Y, DVec3::Z),
            View::Top => (DVec3::NEG_Z, DVec3::Y),
            View::Bottom => (DVec3::Z, DVec3::Y),
            View::Left => (DVec3::X, DVec3::Z),
            View::Right => (DVec3::NEG_X, DVec3::Z),
            View::Custom { dir, up } => (dir, up),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Projection {
    Ortho,
    /// Vertical field of view (of the shorter image side) in degrees.
    Perspective {
        fov_deg: f64,
    },
}

/// Section plane: removes the half space `(q − point) · normal > 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Section {
    pub point: DVec3,
    pub normal: DVec3,
}

impl Section {
    /// Signed distance (positive = removed side).
    pub fn dist(&self, q: DVec3) -> f64 {
        (q - self.point).dot(self.normal.normalize_or_zero())
    }
}

/// One mesh to draw.
#[derive(Debug, Clone)]
pub struct Item<'a> {
    pub mesh: &'a Mesh,
    /// Mesh system → scene (e.g. `Node.world`).
    pub transform: DMat4,
    /// RGBA in 0..1; alpha < 1 draws the mesh transparent.
    pub color: [f32; 4],
    /// Colour of the mesh's edge lines; `None` draws no edges.
    pub edges: Option<[f32; 4]>,
    /// With a section: close the cut with a cap (opaque solids).
    pub cap: bool,
}

impl<'a> Item<'a> {
    pub fn new(mesh: &'a Mesh, color: [f32; 4]) -> Self {
        Item { mesh, transform: DMat4::IDENTITY, color, edges: Some([0.15, 0.15, 0.18, 1.0]), cap: true }
    }

    pub fn with_transform(mut self, m: DMat4) -> Self {
        self.transform = m;
        self
    }

    pub fn with_edges(mut self, c: Option<[f32; 4]>) -> Self {
        self.edges = c;
        self
    }

    pub fn with_cap(mut self, cap: bool) -> Self {
        self.cap = cap;
        self
    }
}

/// A polyline drawn over the shading (depth tested unless `on_top`).
#[derive(Debug, Clone)]
pub struct Polyline {
    pub points: Vec<DVec3>,
    pub color: [f32; 4],
    /// Width in output pixels.
    pub width: f32,
    pub on_top: bool,
    /// Dashed (6 px on, 4 px off).
    pub dashed: bool,
}

impl Polyline {
    pub fn new(points: Vec<DVec3>, color: [f32; 4], width: f32) -> Self {
        Polyline { points, color, width, on_top: false, dashed: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerShape {
    Diamond,
    Dot,
    Square,
}

/// A point marker of fixed screen size.
#[derive(Debug, Clone)]
pub struct Marker {
    pub pos: DVec3,
    pub shape: MarkerShape,
    /// Half size in output pixels.
    pub size: f32,
    pub color: [f32; 4],
    pub on_top: bool,
}

/// Text at a scene point (drawn after everything else; labels that would overlap an already
/// placed one are tried at a few offsets and dropped if none fits).
#[derive(Debug, Clone)]
pub struct Label {
    pub pos: DVec3,
    pub text: String,
    pub color: [f32; 4],
    /// Offset of the text's top left corner from the projected point (output pixels).
    pub offset: (f32, f32),
    /// Higher first.
    pub priority: i32,
}

#[derive(Debug, Clone, Default)]
pub struct Scene<'a> {
    pub items: Vec<Item<'a>>,
    pub lines: Vec<Polyline>,
    pub markers: Vec<Marker>,
    pub labels: Vec<Label>,
}

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub width: u32,
    pub height: u32,
    pub view: View,
    /// Background colour (top of the image with `background_bottom`).
    pub background: [f32; 4],
    /// Vertical gradient: colour at the bottom of the image (`None`: solid `background`).
    pub background_bottom: Option<[f32; 4]>,
    /// Studio look (`None`: the classic per-vertex shading).
    pub studio: Option<Studio>,
    pub axes: bool,
    /// Supersampling factor per axis (1 = none).
    pub supersample: u32,
    /// Free border around the fitted scene as a fraction of the image size.
    pub margin: f64,
    /// Width of mesh edge lines in output pixels.
    pub edge_width: f32,
    pub projection: Projection,
    /// Look-at point (default: centre of the fitted geometry).
    pub target: Option<DVec3>,
    /// Bounds to fit (default: everything drawn).
    pub fit: Option<(DVec3, DVec3)>,
    /// Magnification after fitting.
    pub zoom: f64,
    /// Fixed scale in output pixels per mm at the target (overrides fitting; `zoom` still
    /// applies).
    pub scale: Option<f64>,
    pub section: Option<Section>,
    /// Colour of section caps.
    pub cap_color: [f32; 4],
    /// Text in the upper left corner.
    pub title: Option<String>,
    /// Colour of the title.
    pub title_color: [f32; 4],
    /// Background box behind labels and the title.
    pub label_background: [f32; 4],
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            width: 800,
            height: 600,
            view: View::Iso,
            background: [0.96, 0.96, 0.97, 1.0],
            background_bottom: None,
            studio: None,
            axes: true,
            supersample: 2,
            margin: 0.08,
            edge_width: 1.0,
            projection: Projection::Ortho,
            target: None,
            fit: None,
            zoom: 1.0,
            scale: None,
            section: None,
            cap_color: [0.86, 0.36, 0.24, 1.0],
            title: None,
            title_color: [0.1, 0.1, 0.12, 1.0],
            label_background: [1.0, 1.0, 1.0, 0.78],
        }
    }
}

/// The camera a rendering used (output pixels).
#[derive(Debug, Clone, PartialEq)]
pub struct CameraInfo {
    pub projection: Projection,
    /// Viewing direction (camera → scene), unit.
    pub dir: DVec3,
    pub up: DVec3,
    pub right: DVec3,
    pub target: DVec3,
    /// Camera position (perspective); for orthographic views a point behind the scene.
    pub eye: DVec3,
    /// Output pixels per mm at the target.
    pub scale: f64,
    /// Visible size at the target in mm.
    pub width_mm: f64,
    pub height_mm: f64,
    pub azimuth: f64,
    pub elevation: f64,
    /// What the studio look used (`None` without it).
    pub studio: Option<StudioInfo>,
}

/// 8-bit RGBA image.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }

    /// Number of pixels that differ from `c` by more than 2 in any channel.
    pub fn count_not(&self, c: [u8; 4]) -> usize {
        self.rgba.chunks_exact(4).filter(|p| p.iter().zip(c).any(|(a, b)| (*a as i32 - b as i32).abs() > 2)).count()
    }

    pub fn to_png(&self) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.width, self.height);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().expect("png header");
            w.write_image_data(&self.rgba).expect("png data");
        }
        out
    }

    pub fn write_png(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        std::fs::write(path, self.to_png())
    }
}

pub fn to_u8(c: [f32; 4]) -> [u8; 4] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
}

struct Camera {
    right: DVec3,
    up: DVec3,
    dir: DVec3,
    target: DVec3,
    /// Supersampled pixels per mm at the target.
    scale: f64,
    w: f64,
    h: f64,
    /// Perspective: distance eye → target.
    dist: Option<f64>,
}

impl Camera {
    fn eye(&self) -> DVec3 {
        self.target - self.dir * self.dist.unwrap_or(0.0)
    }

    /// Screen position (pixels, y down) and depth (larger = farther; linear in screen space,
    /// near the target in mm along the viewing direction).
    fn project(&self, p: DVec3) -> (f64, f64, f64) {
        let v = p - self.target;
        match self.dist {
            None => (
                v.dot(self.right) * self.scale + self.w / 2.0,
                self.h / 2.0 - v.dot(self.up) * self.scale,
                v.dot(self.dir),
            ),
            Some(d) => {
                let zc = (d + v.dot(self.dir)).max(1e-9);
                let f = self.scale * d / zc;
                (v.dot(self.right) * f + self.w / 2.0, self.h / 2.0 - v.dot(self.up) * f, d - d * d / zc)
            }
        }
    }

    /// Scene point of sample (sx, sy) at `depth` (inverse of [`Camera::project`]).
    fn unproject(&self, sx: f64, sy: f64, depth: f64) -> DVec3 {
        let (x, y) = (sx - self.w / 2.0, self.h / 2.0 - sy);
        match self.dist {
            None => self.target + self.right * (x / self.scale) + self.up * (y / self.scale) + self.dir * depth,
            Some(d) => {
                let zc = d * d / (d - depth).max(1e-12);
                let f = self.scale * d / zc;
                self.target + self.right * (x / f) + self.up * (y / f) + self.dir * (zc - d)
            }
        }
    }

    /// Linear depth (mm along the viewing direction from the target) of a [`Camera::project`]
    /// depth.
    fn linear_depth(&self, depth: f64) -> f64 {
        match self.dist {
            None => depth,
            Some(d) => d * d / (d - depth).max(1e-12) - d,
        }
    }

    /// Unit direction of the view ray through scene point `p` (camera → `p`).
    fn ray(&self, p: DVec3) -> DVec3 {
        match self.dist {
            None => self.dir,
            Some(_) => (p - self.eye()).normalize_or(self.dir),
        }
    }

    /// Depth (as [`Camera::project`]) where the ray through sample (sx, sy) meets `plane`.
    fn plane_depth(&self, sx: f64, sy: f64, plane: &Section) -> Option<f64> {
        let n = plane.normal.normalize_or_zero();
        let (x, y) = ((sx - self.w / 2.0) / self.scale, (self.h / 2.0 - sy) / self.scale);
        match self.dist {
            None => {
                let o = self.target + self.right * x + self.up * y;
                let dn = self.dir.dot(n);
                if dn.abs() < 1e-9 {
                    return None;
                }
                Some((plane.point - o).dot(n) / dn)
            }
            Some(d) => {
                let eye = self.eye();
                let r = self.right * x + self.up * y + self.dir * d;
                let dn = r.dot(n);
                if dn.abs() < 1e-12 {
                    return None;
                }
                let t = (plane.point - eye).dot(n) / dn;
                if t <= 0.0 {
                    return None;
                }
                Some(self.project(eye + r * t).2)
            }
        }
    }
}

struct Target {
    w: usize,
    h: usize,
    color: Vec<[f32; 3]>,
    depth: Vec<f64>,
}

/// Per-sample surface data of the studio look (deferred shading).
pub(crate) struct GBuf {
    pub albedo: Vec<[f32; 3]>,
    /// Unit normal facing the camera.
    pub normal: Vec<[f32; 3]>,
    /// [`studio::NONE`], [`studio::SURFACE`], [`studio::CAP`] or [`studio::GROUND`].
    pub kind: Vec<u8>,
}

impl Target {
    fn blend(&mut self, i: usize, c: [f32; 3], a: f32) {
        let d = &mut self.color[i];
        for k in 0..3 {
            d[k] = d[k] * (1.0 - a) + c[k] * a;
        }
    }
}

#[derive(Clone, Copy)]
struct SV {
    x: f64,
    y: f64,
    z: f64,
    shade: f32,
    /// Shading normal (camera facing), for the studio look.
    n: [f32; 3],
}

fn edge_fn(a: &SV, b: &SV, x: f64, y: f64) -> f64 {
    (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
}

/// Pixel range of a triangle's screen bounds.
fn tri_box(t: &Target, v: &[SV; 3]) -> Option<(usize, usize, usize, usize)> {
    let minx = v.iter().map(|p| p.x).fold(f64::INFINITY, f64::min).floor().max(0.0) as usize;
    let maxx = v.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max).ceil().min(t.w as f64 - 1.0);
    let miny = v.iter().map(|p| p.y).fold(f64::INFINITY, f64::min).floor().max(0.0) as usize;
    let maxy = v.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max).ceil().min(t.h as f64 - 1.0);
    if maxx < 0.0 || maxy < 0.0 {
        return None;
    }
    Some((minx, maxx as usize, miny, maxy as usize))
}

/// Rasterises a triangle; `f(index, depth, shade)` decides what to do with each covered sample.
fn raster_tri(t: &Target, v: [SV; 3], mut f: impl FnMut(usize, f64, f32)) {
    raster_tri_w(t, v, |i, z, w| {
        f(i, z, (w[0] as f32) * v[0].shade + (w[1] as f32) * v[1].shade + (w[2] as f32) * v[2].shade)
    });
}

/// Rasterises a triangle; `f(index, depth, barycentric weights)` per covered sample.
fn raster_tri_w(t: &Target, v: [SV; 3], mut f: impl FnMut(usize, f64, [f64; 3])) {
    let area = edge_fn(&v[0], &v[1], v[2].x, v[2].y);
    if area.abs() < 1e-12 {
        return;
    }
    let Some((minx, maxx, miny, maxy)) = tri_box(t, &v) else { return };
    for y in miny..=maxy {
        let py = y as f64 + 0.5;
        for x in minx..=maxx {
            let px = x as f64 + 0.5;
            let w0 = edge_fn(&v[1], &v[2], px, py) / area;
            let w1 = edge_fn(&v[2], &v[0], px, py) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                continue;
            }
            let z = w0 * v[0].z + w1 * v[1].z + w2 * v[2].z;
            f(y * t.w + x, z, [w0, w1, w2]);
        }
    }
}

/// Like [`raster_tri`] (without shade) but with a top-left fill rule: a sample on an edge
/// shared by two triangles is covered by exactly one of them (section cap parity).
fn raster_tri_exact(t: &Target, v: [SV; 3], mut f: impl FnMut(usize, f64)) {
    let mut v = v;
    let mut area = edge_fn(&v[0], &v[1], v[2].x, v[2].y);
    if area.abs() < 1e-12 {
        return;
    }
    if area < 0.0 {
        v.swap(1, 2);
        area = -area;
    }
    let Some((minx, maxx, miny, maxy)) = tri_box(t, &v) else { return };
    // y down: with positive area the triangle is clockwise on screen; an edge a→b is "top" if
    // horizontal and going right, "left" if going up.
    let top_left = |a: &SV, b: &SV| (a.y == b.y && b.x > a.x) || b.y < a.y;
    let edges = [(1usize, 2usize), (2, 0), (0, 1)];
    let tl: [bool; 3] = std::array::from_fn(|k| top_left(&v[edges[k].0], &v[edges[k].1]));
    for y in miny..=maxy {
        let py = y as f64 + 0.5;
        for x in minx..=maxx {
            let px = x as f64 + 0.5;
            let mut w = [0.0; 3];
            let mut inside = true;
            for k in 0..3 {
                let e = edge_fn(&v[edges[k].0], &v[edges[k].1], px, py);
                if e < 0.0 || (e == 0.0 && !tl[k]) {
                    inside = false;
                    break;
                }
                w[k] = e / area;
            }
            if inside {
                f(y * t.w + x, w[0] * v[0].z + w[1] * v[1].z + w[2] * v[2].z);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_line(
    t: &mut Target,
    a: (f64, f64, f64),
    b: (f64, f64, f64),
    width: f64,
    c: [f32; 4],
    bias: f64,
    test: bool,
    dash: Option<(f64, f64)>,
) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy);
    let steps = (len * 2.0).ceil().max(1.0) as usize;
    let r = (width / 2.0).max(0.5);
    let ri = r.ceil() as i64;
    for s in 0..=steps {
        let u = s as f64 / steps as f64;
        if let Some((on, period)) = dash
            && (u * len) % period > on
        {
            continue;
        }
        let (x, y, z) = (a.0 + dx * u, a.1 + dy * u, a.2 + (b.2 - a.2) * u);
        for oy in -ri..=ri {
            for ox in -ri..=ri {
                let (qx, qy) = (x + ox as f64, y + oy as f64);
                if ((ox * ox + oy * oy) as f64) > r * r + 0.25 {
                    continue;
                }
                if qx < 0.0 || qy < 0.0 || qx >= t.w as f64 || qy >= t.h as f64 {
                    continue;
                }
                let i = qy as usize * t.w + qx as usize;
                if test && z > t.depth[i] + bias {
                    continue;
                }
                t.blend(i, [c[0], c[1], c[2]], c[3]);
            }
        }
    }
}

struct Tri {
    v: [SV; 3],
    color: [f32; 4],
    /// Index of the scene item (orders coplanar transparent fragments).
    item: u32,
}

/// One transparent sample: depth, triangle, interpolated shade.
#[derive(Clone, Copy)]
struct Frag {
    z: f64,
    tri: u32,
    shade: f32,
}

/// Blends the transparent triangles over the opaque image, sorted back to front **per
/// sample**. Sorting whole triangles by their centroid depth (as before) is wrong wherever
/// triangles overlap in depth: large triangles of a box's faces, and above all coplanar faces
/// of different meshes (a subtracted body and the cavity it leaves in the dimmed result).
/// Their order then flips along triangle diagonals, which showed up as diagonal bands across
/// the faces. Fragments closer in depth than `range * 1e-6` count as coplanar and are blended
/// in scene item order (later items on top), so coincident faces get one consistent colour.
fn blend_transparent(t: &mut Target, tris: &[Tri], range: f64) {
    if tris.is_empty() {
        return;
    }
    // two passes: count fragments per sample, then fill a flat array (counting sort by sample)
    let mut count = vec![0u32; t.w * t.h];
    for tri in tris {
        raster_tri(t, tri.v, |i, z, _| {
            if z < t.depth[i] {
                count[i] += 1;
            }
        });
    }
    let mut start = vec![0usize; t.w * t.h + 1];
    for i in 0..t.w * t.h {
        start[i + 1] = start[i] + count[i] as usize;
    }
    let mut frags = vec![Frag { z: 0.0, tri: 0, shade: 0.0 }; start[t.w * t.h]];
    let mut fill = start.clone();
    for (k, tri) in tris.iter().enumerate() {
        raster_tri(t, tri.v, |i, z, s| {
            if z < t.depth[i] {
                frags[fill[i]] = Frag { z, tri: k as u32, shade: s };
                fill[i] += 1;
            }
        });
    }
    let eps = (range * 1e-6).max(1e-12);
    for i in 0..t.w * t.h {
        let f = &mut frags[start[i]..start[i + 1]];
        if f.is_empty() {
            continue;
        }
        // far to near; coplanar runs by item (lower item first, later items end on top)
        f.sort_by(|a, b| b.z.partial_cmp(&a.z).unwrap_or(std::cmp::Ordering::Equal));
        let mut a = 0;
        while a < f.len() {
            let mut b = a + 1;
            while b < f.len() && f[b - 1].z - f[b].z < eps {
                b += 1;
            }
            if b - a > 1 {
                f[a..b].sort_by_key(|x| (tris[x.tri as usize].item, x.tri));
            }
            a = b;
        }
        for x in f.iter() {
            let c = tris[x.tri as usize].color;
            let s = x.shade;
            t.blend(i, [c[0] * s, c[1] * s, c[2] * s], c[3]);
        }
    }
}

fn v3(p: &[f32]) -> DVec3 {
    DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)
}

/// Clips a convex polygon of (position, normal) against the planes (keeps `dist ≤ 0`).
fn clip_poly(mut poly: Vec<(DVec3, DVec3)>, planes: &[Section]) -> Vec<(DVec3, DVec3)> {
    for pl in planes {
        if poly.is_empty() {
            break;
        }
        let d: Vec<f64> = poly.iter().map(|(p, _)| pl.dist(*p)).collect();
        if d.iter().all(|&x| x <= 0.0) {
            continue;
        }
        let mut out = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let j = (i + 1) % poly.len();
            let (a, b) = (poly[i], poly[j]);
            if d[i] <= 0.0 {
                out.push(a);
            }
            if (d[i] <= 0.0) != (d[j] <= 0.0) {
                let u = d[i] / (d[i] - d[j]);
                out.push((a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u));
            }
        }
        poly = out;
    }
    poly
}

/// Clips a segment against the planes (keeps `dist ≤ 0`).
fn clip_segment(mut a: DVec3, mut b: DVec3, planes: &[Section]) -> Option<(DVec3, DVec3)> {
    for pl in planes {
        let (da, db) = (pl.dist(a), pl.dist(b));
        match (da <= 0.0, db <= 0.0) {
            (true, true) => {}
            (false, false) => return None,
            (true, false) => b = a + (b - a) * (da / (da - db)),
            (false, true) => a = a + (b - a) * (da / (da - db)),
        }
    }
    Some((a, b))
}

/// World points used for fitting (mesh vertices, lines, markers, or the given bounds).
fn fit_points(scene: &Scene, opts: &RenderOptions) -> Vec<DVec3> {
    if let Some((lo, hi)) = opts.fit {
        return (0..8)
            .map(|i| {
                DVec3::new(
                    if i & 1 == 0 { lo.x } else { hi.x },
                    if i & 2 == 0 { lo.y } else { hi.y },
                    if i & 4 == 0 { lo.z } else { hi.z },
                )
            })
            .collect();
    }
    let mut pts = Vec::new();
    for it in &scene.items {
        for p in it.mesh.positions.chunks_exact(3) {
            pts.push(it.transform.transform_point3(v3(p)));
        }
    }
    for l in &scene.lines {
        pts.extend_from_slice(&l.points);
    }
    pts.extend(scene.markers.iter().map(|m| m.pos));
    pts
}

/// Basis (dir, up, right) of a view.
fn basis(view: &View) -> (DVec3, DVec3, DVec3) {
    let (dir, up0) = view.basis();
    let dir = dir.normalize();
    let right = dir.cross(up0).normalize_or(DVec3::X);
    let up = right.cross(dir);
    (dir, up, right)
}

/// Fitted camera for supersampled size (w, h).
fn camera(scene: &Scene, opts: &RenderOptions, w: f64, h: f64) -> Camera {
    let (dir, up, right) = basis(&opts.view);
    let pts = fit_points(scene, opts);
    let empty = pts.is_empty();
    let target = opts.target.unwrap_or_else(|| {
        if empty {
            return DVec3::ZERO;
        }
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for p in &pts {
            let q = DVec3::new(p.dot(right), p.dot(up), p.dot(dir));
            lo = lo.min(q);
            hi = hi.max(q);
        }
        let c = (lo + hi) / 2.0;
        right * c.x + up * c.y + dir * c.z
    });
    let avail = (1.0 - 2.0 * opts.margin).clamp(0.1, 1.0);
    let ss = opts.supersample.max(1) as f64;
    let zoom = if opts.zoom.is_finite() && opts.zoom > 0.0 { opts.zoom } else { 1.0 };
    match opts.projection {
        Projection::Ortho => {
            let (mut hx, mut hy) = (0.0f64, 0.0f64);
            for p in &pts {
                let v = *p - target;
                hx = hx.max(v.dot(right).abs());
                hy = hy.max(v.dot(up).abs());
            }
            let (hx, hy) = (hx.max(1e-9), hy.max(1e-9));
            let fit = if empty { 1.0 } else { (w * avail / (2.0 * hx)).min(h * avail / (2.0 * hy)) };
            let scale = opts.scale.map(|s| s * ss).unwrap_or(fit) * zoom;
            Camera { right, up, dir, target, scale, w, h, dist: None }
        }
        Projection::Perspective { fov_deg } => {
            let half = (fov_deg.clamp(1.0, 170.0) / 2.0).to_radians();
            let r = pts.iter().map(|p| (*p - target).length()).fold(0.0, f64::max).max(1e-6);
            let dist = r / half.sin() / avail;
            let fit = (w.min(h) / 2.0) / (dist * half.tan());
            let scale = opts.scale.map(|s| s * ss).unwrap_or(if empty { 1.0 } else { fit }) * zoom;
            Camera { right, up, dir, target, scale, w, h, dist: Some(dist) }
        }
    }
}

/// Output pixels per mm at the target the fit would use for `opts` (without `scale`).
pub fn fit_scale(scene: &Scene, opts: &RenderOptions) -> f64 {
    let o = RenderOptions { scale: None, zoom: 1.0, supersample: 1, ..opts.clone() };
    camera(scene, &o, opts.width.max(1) as f64, opts.height.max(1) as f64).scale
}

fn camera_info(cam: &Camera, opts: &RenderOptions, ss: f64) -> CameraInfo {
    let scale = cam.scale / ss;
    let eye = match cam.dist {
        Some(_) => cam.eye(),
        None => cam.target - cam.dir * (cam.w.max(cam.h) / cam.scale),
    };
    let (azimuth, elevation) = View::Custom { dir: cam.dir, up: cam.up }.angles();
    CameraInfo {
        projection: opts.projection,
        dir: cam.dir,
        up: cam.up,
        right: cam.right,
        target: cam.target,
        eye,
        scale,
        width_mm: opts.width as f64 / scale,
        height_mm: opts.height as f64 / scale,
        azimuth,
        elevation,
        studio: None,
    }
}

/// Renders the scene.
pub fn render(scene: &Scene, opts: &RenderOptions) -> Image {
    render_info(scene, opts).0
}

/// Renders the scene and returns the camera it used.
pub fn render_info(scene: &Scene, opts: &RenderOptions) -> (Image, CameraInfo) {
    let ss = opts.supersample.max(1) as usize;
    let (w, h) = (opts.width.max(1) as usize * ss, opts.height.max(1) as usize * ss);
    let cam = camera(scene, opts, w as f64, h as f64);
    let (dir, up, right) = (cam.dir, cam.up, cam.right);

    let pts = fit_points(scene, &RenderOptions { fit: None, ..opts.clone() });
    let (mut zlo, mut zhi) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in &pts {
        let z = cam.project(*p).2;
        zlo = zlo.min(z);
        zhi = zhi.max(z);
    }
    if zlo > zhi {
        (zlo, zhi) = (0.0, 0.0);
    }
    let row_bg = |y: usize| background_at(opts, y, h);
    let mut color = Vec::with_capacity(w * h);
    for y in 0..h {
        color.extend(std::iter::repeat_n(row_bg(y), w));
    }
    let mut t = Target { w, h, color, depth: vec![f64::INFINITY; w * h] };
    let studio = opts.studio.as_ref();
    let mut gbuf = studio.map(|_| GBuf {
        albedo: vec![[0.0; 3]; w * h],
        normal: vec![[0.0; 3]; w * h],
        kind: vec![studio::NONE; w * h],
    });
    let bias = ((zhi - zlo).abs() * 2e-3).max(1e-6) + 1.5 * ss as f64 / cam.scale;

    // Clip planes: the section and (perspective) the near plane.
    let mut planes: Vec<Section> = Vec::new();
    if let Some(s) = opts.section
        && s.normal.length() > 1e-12
    {
        planes.push(s);
    }
    let near_plane = cam.dist.map(|d| Section { point: cam.eye() + dir * (d * 0.02), normal: -dir });
    let mut all_planes = planes.clone();
    all_planes.extend(near_plane);

    // Light from the upper left, slightly towards the viewer.
    let light = (-dir * 0.8 + up * 0.6 - right * 0.35).normalize();
    let shade = |n: DVec3| -> f32 { (0.38 + 0.62 * n.dot(light).max(0.0)) as f32 };
    let sv = |p: DVec3, n: DVec3| {
        let (x, y, z) = cam.project(p);
        SV { x, y, z, shade: shade(n), n: n.as_vec3().to_array() }
    };

    let mut transparent: Vec<Tri> = Vec::new();
    let mut opaque: Vec<Tri> = Vec::new();
    // studio: world triangles of the opaque meshes (shadow casters) and the bounds of all items
    let mut casters: Vec<[DVec3; 3]> = Vec::new();
    let (mut mlo, mut mhi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for (item, it) in scene.items.iter().enumerate() {
        let m = &it.mesh;
        let nm = it.transform.inverse().transpose();
        let has_n = m.normals.len() == m.positions.len();
        for tri in m.indices.chunks_exact(3) {
            let p: [DVec3; 3] =
                std::array::from_fn(|k| it.transform.transform_point3(v3(&m.positions[tri[k] as usize * 3..])));
            if studio.is_some() {
                for q in &p {
                    mlo = mlo.min(*q);
                    mhi = mhi.max(*q);
                }
            }
            let fnrm = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
            let vn: [DVec3; 3] = std::array::from_fn(|k| {
                if has_n {
                    nm.transform_vector3(v3(&m.normals[tri[k] as usize * 3..])).normalize_or(fnrm)
                } else {
                    fnrm
                }
            });
            let poly = clip_poly((0..3).map(|k| (p[k], vn[k])).collect(), &all_planes);
            if poly.len() < 3 {
                continue;
            }
            for f in 1..poly.len() - 1 {
                let q = [poly[0], poly[f], poly[f + 1]];
                // two-sided: make the normal face the camera (per view ray in perspective)
                let v: [SV; 3] = std::array::from_fn(|k| {
                    let ray = match cam.dist {
                        Some(_) => (q[k].0 - cam.eye()).normalize_or(dir),
                        None => dir,
                    };
                    let flip = fnrm.dot(ray) > 0.0;
                    let mut n = q[k].1;
                    if (n.dot(ray) > 0.0) != flip || n == DVec3::ZERO {
                        n = fnrm;
                    }
                    if flip {
                        n = -n;
                    }
                    sv(q[k].0, n)
                });
                let t = Tri { v, color: it.color, item: item as u32 };
                if it.color[3] < 0.999 {
                    transparent.push(t);
                } else {
                    if studio.is_some() {
                        casters.push([q[0].0, q[1].0, q[2].0]);
                    }
                    opaque.push(t);
                }
            }
        }
    }
    for tri in &opaque {
        let c = tri.color;
        let mut writes = Vec::new();
        raster_tri_w(&t, tri.v, |i, z, w| {
            if z < t.depth[i] {
                writes.push((i, z, w));
            }
        });
        let v = &tri.v;
        for (i, z, wt) in writes {
            if z < t.depth[i] {
                t.depth[i] = z;
                match gbuf.as_mut() {
                    None => {
                        let s = (wt[0] as f32) * v[0].shade + (wt[1] as f32) * v[1].shade + (wt[2] as f32) * v[2].shade;
                        t.color[i] = [c[0] * s, c[1] * s, c[2] * s];
                    }
                    Some(g) => {
                        let n: [f32; 3] = std::array::from_fn(|k| {
                            (wt[0] as f32) * v[0].n[k] + (wt[1] as f32) * v[1].n[k] + (wt[2] as f32) * v[2].n[k]
                        });
                        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
                        g.normal[i] = [n[0] / l, n[1] / l, n[2] / l];
                        g.albedo[i] = [c[0], c[1], c[2]];
                        g.kind[i] = studio::SURFACE;
                    }
                }
            }
        }
    }

    // Section caps (stencil principle, see the module docs) and the cut contour.
    let mut contour: Vec<(DVec3, DVec3, [f32; 4])> = Vec::new();
    if let Some(sec) = planes.first().copied() {
        let pd: Vec<f64> = (0..w * h)
            .map(|i| cam.plane_depth((i % w) as f64 + 0.5, (i / w) as f64 + 0.5, &sec).unwrap_or(f64::NAN))
            .collect();
        let n = sec.normal.normalize();
        let cap_n = if n.dot(dir) > 0.0 { -n } else { n };
        // caps stay bright from every side (they are the point of a section)
        let cs = 0.72 + 0.28 * cap_n.dot(light).max(0.0) as f32;
        let cc = opts.cap_color;
        let mut parity = vec![false; w * h];
        let oh_cap = h / ss + 7;
        for (ci, it) in scene.items.iter().filter(|i| i.cap && i.color[3] >= 0.999).enumerate() {
            parity.iter_mut().for_each(|p| *p = false);
            let m = &it.mesh;
            let mut any = false;
            for tri in m.indices.chunks_exact(3) {
                let p: [DVec3; 3] =
                    std::array::from_fn(|k| it.transform.transform_point3(v3(&m.positions[tri[k] as usize * 3..])));
                let d: [f64; 3] = std::array::from_fn(|k| sec.dist(p[k]));
                // contour: the triangle crosses the plane
                let mut cut = Vec::new();
                for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                    if (d[a] <= 0.0) != (d[b] <= 0.0) {
                        cut.push(p[a] + (p[b] - p[a]) * (d[a] / (d[a] - d[b])));
                    }
                }
                if cut.len() == 2 {
                    any = true;
                    if let Some(ec) = it.edges {
                        contour.push((cut[0], cut[1], ec));
                    }
                }
                let poly = clip_poly(p.iter().map(|q| (*q, DVec3::ZERO)).collect(), near_plane.as_slice());
                if poly.len() < 3 {
                    continue;
                }
                for f in 1..poly.len() - 1 {
                    let v = [sv(poly[0].0, DVec3::Z), sv(poly[f].0, DVec3::Z), sv(poly[f + 1].0, DVec3::Z)];
                    raster_tri_exact(&t, v, |i, z| {
                        if z < pd[i] {
                            parity[i] = !parity[i];
                        }
                    });
                }
            }
            if !any {
                continue;
            }
            for i in 0..w * h {
                if parity[i] && pd[i] < t.depth[i] {
                    t.depth[i] = pd[i];
                    let (x, y) = (i % w / ss, i / w / ss);
                    // hatching, alternating direction per capped item (neighbouring parts)
                    let hatch = if ci % 2 == 0 { x + y } else { x + 7 * oh_cap - y };
                    let k = if hatch % 7 == 0 { 0.72 } else { 1.0 };
                    t.color[i] = [cc[0] * cs * k, cc[1] * cs * k, cc[2] * cs * k];
                    if let Some(g) = gbuf.as_mut() {
                        g.albedo[i] = [cc[0] * k, cc[1] * k, cc[2] * k];
                        g.normal[i] = cap_n.as_vec3().to_array();
                        g.kind[i] = studio::CAP;
                    }
                }
            }
        }
    }

    // Studio look: ground, ambient occlusion, shadows and lighting of the opaque samples.
    let mut studio_info = None;
    if let (Some(st), Some(g)) = (studio, gbuf.as_mut()) {
        let bounds = if mlo.x <= mhi.x { Some((mlo, mhi)) } else { None };
        studio_info = Some(studio::shade(&mut t, g, &cam, opts, st, &casters, bounds));
    }

    // Edges of opaque meshes (depth tested).
    let lw = opts.edge_width as f64 * ss as f64;
    let draw_edges = |t: &mut Target, it: &Item, ec: [f32; 4]| {
        for seg in it.mesh.edge_segments.chunks_exact(6) {
            let a = it.transform.transform_point3(v3(&seg[0..3]));
            let b = it.transform.transform_point3(v3(&seg[3..6]));
            if let Some((a, b)) = clip_segment(a, b, &all_planes) {
                draw_line(t, cam.project(a), cam.project(b), lw, ec, bias, true, None);
            }
        }
    };
    // studio: lighter edges (the shading shows the form)
    let edge_alpha = if studio.is_some() { studio::EDGE_ALPHA } else { 1.0 };
    for it in scene.items.iter().filter(|i| i.color[3] >= 0.999) {
        if let Some(ec) = it.edges {
            draw_edges(&mut t, it, [ec[0], ec[1], ec[2], ec[3] * edge_alpha]);
        }
    }
    for (a, b, c) in &contour {
        if let Some((a, b)) = clip_segment(*a, *b, near_plane.as_slice()) {
            draw_line(&mut t, cam.project(a), cam.project(b), lw, *c, bias, true, None);
        }
    }
    blend_transparent(&mut t, &transparent, (zhi - zlo).abs());
    for it in scene.items.iter().filter(|i| i.color[3] < 0.999) {
        if let Some(ec) = it.edges {
            let ec = [ec[0], ec[1], ec[2], ec[3] * it.color[3].max(0.3)];
            draw_edges(&mut t, it, ec);
        }
    }
    let dash = Some((6.0 * ss as f64, 10.0 * ss as f64));
    for l in &scene.lines {
        for w2 in l.points.windows(2) {
            let Some((a, b)) = clip_segment(w2[0], w2[1], near_plane.as_slice()) else { continue };
            let (a, b) = (cam.project(a), cam.project(b));
            draw_line(
                &mut t,
                a,
                b,
                l.width as f64 * ss as f64,
                l.color,
                bias,
                !l.on_top,
                if l.dashed { dash } else { None },
            );
        }
    }
    for mk in &scene.markers {
        if near_plane.is_some_and(|n| n.dist(mk.pos) > 0.0) {
            continue;
        }
        let (x, y, z) = cam.project(mk.pos);
        let s = mk.size as f64 * ss as f64;
        let si = s.ceil() as i64 + 1;
        let outline = [mk.color[0] * 0.35, mk.color[1] * 0.35, mk.color[2] * 0.35, mk.color[3]];
        for oy in -si..=si {
            for ox in -si..=si {
                let (qx, qy) = (x + ox as f64, y + oy as f64);
                if qx < 0.0 || qy < 0.0 || qx >= w as f64 || qy >= h as f64 {
                    continue;
                }
                let (dx, dy) = ((ox as f64).abs(), (oy as f64).abs());
                let r = match mk.shape {
                    MarkerShape::Diamond => dx + dy,
                    MarkerShape::Dot => dx.hypot(dy),
                    MarkerShape::Square => dx.max(dy),
                };
                if r > s {
                    continue;
                }
                let i = qy as usize * w + qx as usize;
                if !mk.on_top && z > t.depth[i] + bias {
                    continue;
                }
                let c = if r > s - 1.2 * ss as f64 { outline } else { mk.color };
                t.blend(i, [c[0], c[1], c[2]], c[3]);
            }
        }
    }
    if opts.axes {
        let len = 28.0 * ss as f64;
        let o = (18.0 * ss as f64 + len, h as f64 - 18.0 * ss as f64 - len, 0.0);
        let axes =
            [(DVec3::X, [0.85, 0.2, 0.2, 1.0]), (DVec3::Y, [0.2, 0.65, 0.25, 1.0]), (DVec3::Z, [0.2, 0.35, 0.9, 1.0])];
        // farthest first so nearer axes are drawn on top
        let mut order: Vec<_> = axes.iter().collect();
        order.sort_by(|a, b| b.0.dot(dir).partial_cmp(&a.0.dot(dir)).unwrap_or(std::cmp::Ordering::Equal));
        for (ax, c) in order {
            let e = (o.0 + ax.dot(right) * len, o.1 - ax.dot(up) * len, 0.0);
            draw_line(&mut t, o, e, 2.0 * ss as f64, *c, 0.0, false, None);
        }
    }

    // Downsample.
    let (ow, oh) = (opts.width.max(1) as usize, opts.height.max(1) as usize);
    let mut rgba = vec![0u8; ow * oh * 4];
    let n = (ss * ss) as f32;
    for y in 0..oh {
        for x in 0..ow {
            let mut acc = [0.0f32; 3];
            for sy in 0..ss {
                for sx in 0..ss {
                    let c = t.color[(y * ss + sy) * w + x * ss + sx];
                    for k in 0..3 {
                        acc[k] += c[k];
                    }
                }
            }
            let px = to_u8([acc[0] / n, acc[1] / n, acc[2] / n, 1.0]);
            rgba[(y * ow + x) * 4..(y * ow + x) * 4 + 4].copy_from_slice(&px);
        }
    }
    let mut img = Image { width: ow as u32, height: oh as u32, rgba };

    // Labels (output pixels), highest priority first, without overlaps.
    let mut placed: Vec<(i64, i64, i64, i64)> = Vec::new();
    if let Some(title) = &opts.title {
        let b = img.draw_label_on(8, 6, title, TextSize::Large, opts.title_color, opts.label_background);
        placed.push((b.0, b.1, b.0 + b.2 as i64, b.1 + b.3 as i64));
    }
    let mut labels: Vec<&Label> = scene.labels.iter().collect();
    labels.sort_by_key(|l| std::cmp::Reverse(l.priority));
    for l in labels {
        if near_plane.is_some_and(|n| n.dist(l.pos) > 0.0) {
            continue;
        }
        let (x, y, _) = cam.project(l.pos);
        let (x, y) = (x / ss as f64, y / ss as f64);
        let (tw, th) = text_size(&l.text, TextSize::Small);
        let (tw, th) = (tw as i64 + 4, th as i64 + 2);
        let (ox, oy) = (l.offset.0 as i64, l.offset.1 as i64);
        let cands = [(ox, oy), (ox, -oy - th), (-ox - tw, oy), (-ox - tw, -oy - th), (ox, oy + th), (ox, -oy - 2 * th)];
        for (dx, dy) in cands {
            let (bx, by) = (x as i64 + dx, y as i64 + dy);
            let r = (bx - 2, by - 1, bx + tw - 2, by + th - 1);
            if r.0 < 0 || r.1 < 0 || r.2 > ow as i64 || r.3 > oh as i64 {
                continue;
            }
            // 3 px apart horizontally, 2 px vertically: touching boxes read as one label
            if placed.iter().any(|p| r.0 < p.2 + 3 && p.0 < r.2 + 3 && r.1 < p.3 + 2 && p.1 < r.3 + 2) {
                continue;
            }
            img.draw_label_on(bx, by, &l.text, TextSize::Small, l.color, opts.label_background);
            placed.push(r);
            break;
        }
    }
    let mut info = camera_info(&cam, opts, ss as f64);
    info.studio = studio_info;
    (img, info)
}

/// Background colour of sample row `y` of `h` (gradient from `background` to
/// `background_bottom`).
fn background_at(opts: &RenderOptions, y: usize, h: usize) -> [f32; 3] {
    let top = opts.background;
    match opts.background_bottom {
        None => [top[0], top[1], top[2]],
        Some(b) => {
            let u = if h > 1 { y as f32 / (h - 1) as f32 } else { 0.0 };
            std::array::from_fn(|k| top[k] + (b[k] - top[k]) * u)
        }
    }
}
