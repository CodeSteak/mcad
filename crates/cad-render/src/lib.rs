//! Software rasterizer for the CLI (`render`): meshes + camera → RGBA image → PNG.
//!
//! Orthographic camera with standard views, fitted to the scene. Z-buffer, per-vertex Lambert
//! shading (two-sided), edge lines from [`Mesh::edge_segments`] with depth test, colour per
//! mesh, transparency (transparent fragments are sorted back to front per sample and blended over the
//! opaque image), extra polylines (e.g. selected edges), optional axis triad.
//! Anti-aliasing by supersampling. No GPU.

use cad_kernel::Mesh;
use glam::{DMat4, DVec3};

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

    /// (viewing direction, up)
    fn basis(&self) -> (DVec3, DVec3) {
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
}

impl<'a> Item<'a> {
    pub fn new(mesh: &'a Mesh, color: [f32; 4]) -> Self {
        Item { mesh, transform: DMat4::IDENTITY, color, edges: Some([0.15, 0.15, 0.18, 1.0]) }
    }

    pub fn with_transform(mut self, m: DMat4) -> Self {
        self.transform = m;
        self
    }

    pub fn with_edges(mut self, c: Option<[f32; 4]>) -> Self {
        self.edges = c;
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
}

#[derive(Debug, Clone, Default)]
pub struct Scene<'a> {
    pub items: Vec<Item<'a>>,
    pub lines: Vec<Polyline>,
}

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub width: u32,
    pub height: u32,
    pub view: View,
    pub background: [f32; 4],
    pub axes: bool,
    /// Supersampling factor per axis (1 = none).
    pub supersample: u32,
    /// Free border around the fitted scene as a fraction of the image size.
    pub margin: f64,
    /// Width of mesh edge lines in output pixels.
    pub edge_width: f32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            width: 800,
            height: 600,
            view: View::Iso,
            background: [0.96, 0.96, 0.97, 1.0],
            axes: true,
            supersample: 2,
            margin: 0.08,
            edge_width: 1.0,
        }
    }
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
    scale: f64,
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
}

impl Camera {
    /// Screen position (pixels, y down) and depth (larger = farther).
    fn project(&self, p: DVec3) -> (f64, f64, f64) {
        let x = (p.dot(self.right) - self.cx) * self.scale + self.w / 2.0;
        let y = self.h / 2.0 - (p.dot(self.up) - self.cy) * self.scale;
        (x, y, p.dot(self.dir))
    }
}

struct Target {
    w: usize,
    h: usize,
    color: Vec<[f32; 3]>,
    depth: Vec<f64>,
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
}

fn edge_fn(a: &SV, b: &SV, x: f64, y: f64) -> f64 {
    (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
}

/// Rasterises a triangle; `f(index, depth, shade)` decides what to do with each covered sample.
fn raster_tri(t: &Target, v: [SV; 3], mut f: impl FnMut(usize, f64, f32)) {
    let area = edge_fn(&v[0], &v[1], v[2].x, v[2].y);
    if area.abs() < 1e-12 {
        return;
    }
    let minx = v.iter().map(|p| p.x).fold(f64::INFINITY, f64::min).floor().max(0.0) as usize;
    let maxx = v.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max).ceil().min(t.w as f64 - 1.0);
    let miny = v.iter().map(|p| p.y).fold(f64::INFINITY, f64::min).floor().max(0.0) as usize;
    let maxy = v.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max).ceil().min(t.h as f64 - 1.0);
    if maxx < 0.0 || maxy < 0.0 {
        return;
    }
    let (maxx, maxy) = (maxx as usize, maxy as usize);
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
            let s = (w0 as f32) * v[0].shade + (w1 as f32) * v[1].shade + (w2 as f32) * v[2].shade;
            f(y * t.w + x, z, s);
        }
    }
}

fn draw_line(t: &mut Target, a: (f64, f64, f64), b: (f64, f64, f64), width: f64, c: [f32; 4], bias: f64, test: bool) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy);
    let steps = (len * 2.0).ceil().max(1.0) as usize;
    let r = (width / 2.0).max(0.5);
    let ri = r.ceil() as i64;
    for s in 0..=steps {
        let u = s as f64 / steps as f64;
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

/// Renders the scene.
pub fn render(scene: &Scene, opts: &RenderOptions) -> Image {
    let ss = opts.supersample.max(1) as usize;
    let (w, h) = (opts.width.max(1) as usize * ss, opts.height.max(1) as usize * ss);
    let (dir, up0) = opts.view.basis();
    let dir = dir.normalize();
    let right = dir.cross(up0).normalize_or(DVec3::X);
    let up = right.cross(dir);

    // Fit: projected bounds of all geometry.
    let mut lo = (f64::INFINITY, f64::INFINITY);
    let mut hi = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut zlo = f64::INFINITY;
    let mut zhi = f64::NEG_INFINITY;
    let mut add = |p: DVec3| {
        let (x, y) = (p.dot(right), p.dot(up));
        lo = (lo.0.min(x), lo.1.min(y));
        hi = (hi.0.max(x), hi.1.max(y));
        zlo = zlo.min(p.dot(dir));
        zhi = zhi.max(p.dot(dir));
    };
    for it in &scene.items {
        for p in it.mesh.positions.chunks_exact(3) {
            add(it.transform.transform_point3(v3(p)));
        }
    }
    for l in &scene.lines {
        l.points.iter().for_each(|p| add(*p));
    }
    let bg = [opts.background[0], opts.background[1], opts.background[2]];
    let mut t = Target { w, h, color: vec![bg; w * h], depth: vec![f64::INFINITY; w * h] };
    let empty = lo.0 > hi.0;
    let (sx, sy) = if empty { (1.0, 1.0) } else { ((hi.0 - lo.0).max(1e-9), (hi.1 - lo.1).max(1e-9)) };
    let avail = (1.0 - 2.0 * opts.margin).clamp(0.1, 1.0);
    let scale = ((w as f64 * avail) / sx).min((h as f64 * avail) / sy);
    let cam = Camera {
        right,
        up,
        dir,
        scale,
        cx: if empty { 0.0 } else { (lo.0 + hi.0) / 2.0 },
        cy: if empty { 0.0 } else { (lo.1 + hi.1) / 2.0 },
        w: w as f64,
        h: h as f64,
    };
    let bias = ((zhi - zlo).abs() * 2e-3).max(1e-6) + 1.5 * ss as f64 / scale;

    // Light from the upper left, slightly towards the viewer.
    let light = (-dir * 0.8 + up * 0.6 - right * 0.35).normalize();
    let shade = |n: DVec3| -> f32 { (0.38 + 0.62 * n.dot(light).max(0.0)) as f32 };

    let mut transparent: Vec<Tri> = Vec::new();
    let mut opaque: Vec<Tri> = Vec::new();
    for (item, it) in scene.items.iter().enumerate() {
        let m = &it.mesh;
        let nm = it.transform.inverse().transpose();
        let has_n = m.normals.len() == m.positions.len();
        for tri in m.indices.chunks_exact(3) {
            let p: [DVec3; 3] =
                std::array::from_fn(|k| it.transform.transform_point3(v3(&m.positions[tri[k] as usize * 3..])));
            let fnrm = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
            // two-sided: make the normal face the camera
            let flip = fnrm.dot(dir) > 0.0;
            let v: [SV; 3] = std::array::from_fn(|k| {
                let mut n = if has_n {
                    nm.transform_vector3(v3(&m.normals[tri[k] as usize * 3..])).normalize_or(fnrm)
                } else {
                    fnrm
                };
                if (n.dot(dir) > 0.0) != flip || n == DVec3::ZERO {
                    n = fnrm;
                }
                if flip {
                    n = -n;
                }
                let (x, y, z) = cam.project(p[k]);
                SV { x, y, z, shade: shade(n) }
            });
            let t = Tri { v, color: it.color, item: item as u32 };
            if it.color[3] < 0.999 {
                transparent.push(t);
            } else {
                opaque.push(t);
            }
        }
    }
    for tri in &opaque {
        let c = tri.color;
        let mut writes = Vec::new();
        raster_tri(&t, tri.v, |i, z, s| {
            if z < t.depth[i] {
                writes.push((i, z, s));
            }
        });
        for (i, z, s) in writes {
            if z < t.depth[i] {
                t.depth[i] = z;
                t.color[i] = [c[0] * s, c[1] * s, c[2] * s];
            }
        }
    }
    // Edges of opaque meshes (depth tested).
    let lw = opts.edge_width as f64 * ss as f64;
    for it in scene.items.iter().filter(|i| i.color[3] >= 0.999) {
        if let Some(ec) = it.edges {
            for seg in it.mesh.edge_segments.chunks_exact(6) {
                let a = cam.project(it.transform.transform_point3(v3(&seg[0..3])));
                let b = cam.project(it.transform.transform_point3(v3(&seg[3..6])));
                draw_line(&mut t, a, b, lw, ec, bias, true);
            }
        }
    }
    blend_transparent(&mut t, &transparent, (zhi - zlo).abs());
    for it in scene.items.iter().filter(|i| i.color[3] < 0.999) {
        if let Some(ec) = it.edges {
            let ec = [ec[0], ec[1], ec[2], ec[3] * it.color[3].max(0.3)];
            for seg in it.mesh.edge_segments.chunks_exact(6) {
                let a = cam.project(it.transform.transform_point3(v3(&seg[0..3])));
                let b = cam.project(it.transform.transform_point3(v3(&seg[3..6])));
                draw_line(&mut t, a, b, lw, ec, bias, true);
            }
        }
    }
    for l in &scene.lines {
        for w2 in l.points.windows(2) {
            let (a, b) = (cam.project(w2[0]), cam.project(w2[1]));
            draw_line(&mut t, a, b, l.width as f64 * ss as f64, l.color, bias, !l.on_top);
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
            draw_line(&mut t, o, e, 2.0 * ss as f64, *c, 0.0, false);
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
    Image { width: ow as u32, height: oh as u32, rgba }
}
