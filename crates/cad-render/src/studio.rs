//! Studio look: deferred shading of the opaque samples after rasterisation.
//!
//! * Lights: a key light whose azimuth is relative to the camera (like a turntable: the model
//!   turns, the lights stay with the camera) and whose elevation is measured from the xy plane
//!   (z up), a weak fill light from the camera, and hemisphere ambient light (sky from +z,
//!   bounce from the ground). Blinn-Phong highlight of the key light on the model.
//! * Shadows: an orthographic shadow map of the opaque (clipped) triangles along the key light,
//!   filtered with percentage-closer soft shadows (blocker search, penumbra from the distance
//!   between blocker and receiver, Vogel disk samples rotated per sample). The model shadows
//!   itself and the ground. Section caps and transparent items cast no shadow.
//! * Ambient occlusion: screen space, from the linear depth buffer: 16 samples in the normal
//!   hemisphere of radius `ao_radius` (mm, default 5 % of the model diagonal), rotated in a 4 × 4
//!   pattern and blurred over 4 × 4 samples with a depth/normal test.
//! * Ground: the plane z = lowest point of the model, only seen from above, faded into the
//!   background around the model. It does not hide overlays (grids, lines, labels).
//! * Background: solid or vertical gradient ([`crate::RenderOptions::background_bottom`]).
//! * Mesh edge lines at half opacity (the shading shows the form).
//!
//! No textures, no environment maps: everything is analytic. Deterministic (the sample
//! rotations are fixed patterns), parallel over image rows.

use glam::DVec3;

use crate::{Camera, GBuf, RenderOptions, Target, background_at};

/// Sample kinds of the G-buffer.
pub(crate) const NONE: u8 = 0;
pub(crate) const SURFACE: u8 = 1;
pub(crate) const CAP: u8 = 2;
pub(crate) const GROUND: u8 = 3;

/// Apparent radius of the key light at `shadow_softness = 1` (degrees).
pub const MAX_LIGHT_RADIUS_DEG: f64 = 5.0;

/// Opacity factor of the mesh edge lines in the studio look.
pub(crate) const EDGE_ALPHA: f32 = 0.35;

/// Settings of the studio look (defaults: [`Studio::default`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Studio {
    /// Key light azimuth in degrees relative to the camera: 0 = from the camera, positive =
    /// from the right, negative = from the left.
    pub light_azimuth: f64,
    /// Key light elevation in degrees above the xy plane (90 = from straight above).
    pub light_elevation: f64,
    /// Strength of the key and fill light (1 = default).
    pub light_intensity: f64,
    /// Share of the ambient (sky) light, 0..1.
    pub ambient: f64,
    pub shadows: bool,
    /// 0 = hard shadow edges, 1 = very soft (apparent light radius 0…5°).
    pub shadow_softness: f64,
    pub ao: bool,
    /// Ambient occlusion radius in mm (`None`: 5 % of the model diagonal).
    pub ao_radius: Option<f64>,
    /// Ambient occlusion strength, 0..2 (1 = default).
    pub ao_strength: f64,
    pub ground: bool,
    pub ground_color: [f32; 4],
}

impl Default for Studio {
    fn default() -> Self {
        Studio {
            light_azimuth: -40.0,
            light_elevation: 50.0,
            light_intensity: 1.0,
            ambient: 0.4,
            shadows: true,
            shadow_softness: 0.35,
            ao: true,
            ao_radius: None,
            ao_strength: 1.0,
            ground: true,
            ground_color: [0.9, 0.905, 0.915, 1.0],
        }
    }
}

/// What a studio rendering used.
#[derive(Debug, Clone, PartialEq)]
pub struct StudioInfo {
    /// Unit vector towards the key light (world).
    pub light_dir: DVec3,
    /// Height of the ground plane (`None`: no ground drawn, e.g. seen from below).
    pub ground_z: Option<f64>,
    /// Ambient occlusion radius in mm (`None`: AO off).
    pub ao_radius: Option<f64>,
    /// Shadow map size in texels (`None`: shadows off or nothing casts).
    pub shadow_map: Option<u32>,
}

/// Direction towards a light at `az` (relative to the camera azimuth) and `el` degrees.
pub fn light_direction(cam_dir: DVec3, cam_up: DVec3, az: f64, el: f64) -> DVec3 {
    let eye = -cam_dir;
    let cam_az = if eye.x.abs() < 1e-9 && eye.y.abs() < 1e-9 {
        // straight from above/below: the viewer stands where the screen's up points away from
        let e = -cam_up * eye.z.signum();
        e.x.atan2(-e.y)
    } else {
        eye.x.atan2(-eye.y)
    };
    let a = cam_az + az.to_radians();
    let e = el.clamp(-89.9, 89.9).to_radians();
    DVec3::new(a.sin() * e.cos(), -a.cos() * e.cos(), e.sin()).normalize()
}

fn v3f(a: [f32; 3]) -> DVec3 {
    DVec3::new(a[0] as f64, a[1] as f64, a[2] as f64)
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Interleaved gradient noise (0..1) of a sample position: rotation of the sample patterns.
fn ign(x: usize, y: usize) -> f64 {
    let f = 0.06711056 * x as f64 + 0.00583715 * y as f64;
    (52.9829189 * (f - f.floor())).fract()
}

const BAYER4: [f64; 16] = [0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0];
const GOLDEN: f64 = 2.399963229728653;

/// Vogel disk point `k` of `n` (radius ≤ 1), rotated by `phi`.
fn vogel(k: usize, n: usize, phi: f64) -> (f64, f64) {
    let r = ((k as f64 + 0.5) / n as f64).sqrt();
    let t = k as f64 * GOLDEN + phi;
    (t.cos() * r, t.sin() * r)
}

/// Runs `f(first_row, rows)` over chunks of rows of `buf` (row length `w`) in parallel.
fn par_rows<T: Send>(buf: &mut [T], w: usize, f: impl Fn(usize, &mut [T]) + Sync) {
    let h = buf.len() / w.max(1);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 16);
    if threads == 1 || h < 32 {
        f(0, buf);
        return;
    }
    let rows = h.div_ceil(threads * 4).max(8);
    let f = &f;
    std::thread::scope(|s| {
        for (k, chunk) in buf.chunks_mut(rows * w).enumerate() {
            s.spawn(move || f(k * rows, chunk));
        }
    });
}

/// Orthographic depth map along the key light.
struct ShadowMap {
    lx: DVec3,
    ly: DVec3,
    lz: DVec3,
    u0: f64,
    v0: f64,
    /// mm per texel.
    texel: f64,
    n: usize,
    depth: Vec<f32>,
    /// Least depth per tile of `TILE` × `TILE` texels (early exit of the blocker search).
    tiles: Vec<f32>,
    /// Least depth (nearest to the light).
    dmin: f64,
}

const TILE: usize = 16;

impl ShadowMap {
    fn build(casters: &[[DVec3; 3]], light: DVec3, n: usize) -> Option<ShadowMap> {
        if casters.is_empty() {
            return None;
        }
        let lz = -light;
        let lx = if lz.z.abs() < 0.9 { DVec3::Z.cross(lz) } else { DVec3::X.cross(lz) }.normalize();
        let ly = lz.cross(lx);
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for t in casters {
            for p in t {
                let q = DVec3::new(p.dot(lx), p.dot(ly), p.dot(lz));
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
        let ext = (hi.x - lo.x).max(hi.y - lo.y).max(1e-6) * 1.04;
        let texel = ext / n as f64;
        let u0 = (lo.x + hi.x) / 2.0 - ext / 2.0;
        let v0 = (lo.y + hi.y) / 2.0 - ext / 2.0;
        let mut sm = ShadowMap {
            lx,
            ly,
            lz,
            u0,
            v0,
            texel,
            n,
            depth: vec![f32::INFINITY; n * n],
            tiles: Vec::new(),
            dmin: lo.z,
        };
        for t in casters {
            let q: [(f64, f64, f64); 3] = std::array::from_fn(|k| sm.coords(t[k]));
            sm.raster(q);
        }
        let nt = n.div_ceil(TILE);
        let mut tiles = vec![f32::INFINITY; nt * nt];
        for y in 0..n {
            for x in 0..n {
                let k = (y / TILE) * nt + x / TILE;
                tiles[k] = tiles[k].min(sm.depth[y * n + x]);
            }
        }
        sm.tiles = tiles;
        Some(sm)
    }

    /// Texel coordinates and depth of a point.
    fn coords(&self, p: DVec3) -> (f64, f64, f64) {
        ((p.dot(self.lx) - self.u0) / self.texel, (p.dot(self.ly) - self.v0) / self.texel, p.dot(self.lz))
    }

    fn raster(&mut self, v: [(f64, f64, f64); 3]) {
        let e =
            |a: (f64, f64, f64), b: (f64, f64, f64), x: f64, y: f64| (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0);
        let area = e(v[0], v[1], v[2].0, v[2].1);
        let n = self.n as f64;
        let minx = v.iter().map(|p| p.0).fold(f64::INFINITY, f64::min).floor().max(0.0);
        let maxx = v.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max).ceil().min(n - 1.0);
        let miny = v.iter().map(|p| p.1).fold(f64::INFINITY, f64::min).floor().max(0.0);
        let maxy = v.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max).ceil().min(n - 1.0);
        if maxx < minx || maxy < miny {
            return;
        }
        if area.abs() < 1e-12 {
            // edge-on to the light: still blocks along its line (thin walls)
            for k in 0..3 {
                let (a, b) = (v[k], v[(k + 1) % 3]);
                let steps = ((b.0 - a.0).abs().max((b.1 - a.1).abs()) * 2.0).ceil().max(1.0) as usize;
                for s in 0..=steps {
                    let u = s as f64 / steps as f64;
                    let (x, y, z) = (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u, a.2 + (b.2 - a.2) * u);
                    if x >= 0.0 && y >= 0.0 && x < n && y < n {
                        let i = y as usize * self.n + x as usize;
                        self.depth[i] = self.depth[i].min(z as f32);
                    }
                }
            }
            return;
        }
        // depth is linear in x and y: z = z0 + zx·x + zy·y
        let (a, b, c) = (v[0], v[1], v[2]);
        let zx = ((b.2 - a.2) * (c.1 - a.1) - (c.2 - a.2) * (b.1 - a.1)) / area;
        let zy = ((c.2 - a.2) * (b.0 - a.0) - (b.2 - a.2) * (c.0 - a.0)) / area;
        let sgn = area.signum();
        let edges = [(v[1], v[2]), (v[2], v[0]), (v[0], v[1])];
        for y in miny as usize..=maxy as usize {
            let py = y as f64 + 0.5;
            // span of the covered texel centres in this row (each edge is a half plane in x)
            let (mut xl, mut xr) = (minx, maxx + 1.0);
            for (p, q) in edges {
                let dd = (q.1 - p.1) * sgn;
                let cc = ((q.0 - p.0) * (py - p.1) + (q.1 - p.1) * p.0) * sgn;
                if dd.abs() < 1e-15 {
                    if cc < -1e-9 {
                        xr = xl - 1.0;
                    }
                } else if dd > 0.0 {
                    xr = xr.min(cc / dd + 1e-9);
                } else {
                    xl = xl.max(cc / dd - 1e-9);
                }
            }
            let x0 = (xl - 0.5).ceil().max(minx) as i64;
            let x1 = (xr - 0.5).floor().min(maxx) as i64;
            let row = y * self.n;
            for x in x0.max(0)..=x1 {
                let px = x as f64 + 0.5;
                let z = (a.2 + zx * (px - a.0) + zy * (py - a.1)) as f32;
                let i = row + x as usize;
                if z < self.depth[i] {
                    self.depth[i] = z;
                }
            }
        }
    }

    fn at(&self, x: f64, y: f64) -> f32 {
        if x < 0.0 || y < 0.0 || x >= self.n as f64 || y >= self.n as f64 {
            return f32::INFINITY;
        }
        self.depth[y as usize * self.n + x as usize]
    }

    /// Lit fraction (0..1) of a receiver at `p` with unit normal `n`, `ndl` = n · light.
    fn visibility(&self, p: DVec3, n: DVec3, ndl: f64, tan_theta: f64, phi: f64) -> f64 {
        // normal offset against acne, more where the light grazes the surface
        let p = p + n * self.texel * (1.0 + 1.5 * (1.0 - ndl.clamp(0.0, 1.0)));
        let (x, y, d) = self.coords(p);
        let nf = self.n as f64;
        if x < 0.0 || y < 0.0 || x >= nf || y >= nf {
            return 1.0;
        }
        // receiver plane: its depth changes by (gx, gy) mm per texel across the map, so the
        // samples of a wide filter compare against the plane, not against one depth
        let nz = ndl.max(0.25);
        let (gx, gy) = ((n.dot(self.lx) / nz).clamp(-4.0, 4.0), (n.dot(self.ly) / nz).clamp(-4.0, 4.0));
        let bias = self.texel * 1.5;
        let receiver = |ox: f64, oy: f64| d + (gx * ox + gy * oy) * self.texel - bias;
        // blocker search
        let rs = ((d - self.dmin).max(0.0) * tan_theta / self.texel).clamp(1.5, nf * 0.06);
        // no texel near enough to the light in the search square: lit
        let nt = self.n.div_ceil(TILE);
        let (tx0, tx1) = (((x - rs).max(0.0) as usize) / TILE, ((x + rs).min(nf - 1.0) as usize) / TILE);
        let (ty0, ty1) = (((y - rs).max(0.0) as usize) / TILE, ((y + rs).min(nf - 1.0) as usize) / TILE);
        let limit = (d - bias - rs * (gx.abs() + gy.abs()) * self.texel) as f32;
        if (ty0..=ty1).all(|ty| (tx0..=tx1).all(|tx| self.tiles[ty * nt + tx] >= limit)) {
            return 1.0;
        }
        const NB: usize = 12;
        let (mut sum, mut cnt) = (0.0, 0usize);
        for k in 0..NB {
            let (ox, oy) = vogel(k, NB, phi);
            let (ox, oy) = (ox * rs, oy * rs);
            let z = self.at(x + ox, y + oy) as f64;
            let r = receiver(ox, oy);
            if z < r {
                // distance blocker → receiver at this sample
                sum += r + bias - z;
                cnt += 1;
            }
        }
        if cnt == 0 {
            return 1.0;
        }
        let gap = sum / cnt as f64;
        let pen = (gap * tan_theta / self.texel).clamp(1.0, rs.max(1.0));
        const NF: usize = 16;
        let mut lit = 0usize;
        for k in 0..NF {
            let (ox, oy) = vogel(k, NF, phi + 1.3);
            let (ox, oy) = (ox * pen, oy * pen);
            if self.at(x + ox, y + oy) as f64 >= receiver(ox, oy) {
                lit += 1;
            }
        }
        lit as f64 / NF as f64
    }
}

/// Soft clip above 0.8 (keeps bright, directly lit faces from burning out).
fn tone(x: f32) -> f32 {
    if x <= 0.8 { x.max(0.0) } else { 0.8 + 0.2 * (1.0 - (-(x - 0.8) / 0.2).exp()) }
}

/// Shades the opaque samples of `t` from the G-buffer (see the module docs).
#[allow(clippy::too_many_arguments)]
pub(crate) fn shade(
    t: &mut Target,
    g: &mut GBuf,
    cam: &Camera,
    opts: &RenderOptions,
    st: &Studio,
    casters: &[[DVec3; 3]],
    bounds: Option<(DVec3, DVec3)>,
) -> StudioInfo {
    let (w, h) = (t.w, t.h);
    let light = light_direction(cam.dir, cam.up, st.light_azimuth, st.light_elevation);
    let (lo, hi) = bounds.unwrap_or((DVec3::ZERO, DVec3::ZERO));
    let diag = (hi - lo).length().max(1e-3);
    let center = (lo + hi) / 2.0;
    let sample = |i: usize| ((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);

    // Ground: samples where the ray meets z = lowest point before any geometry.
    let mut gdepth = vec![f64::INFINITY; w * h];
    let mut ground_z = None;
    if st.ground && bounds.is_some() {
        let z0 = lo.z;
        let plane = crate::Section { point: DVec3::new(0.0, 0.0, z0), normal: DVec3::Z };
        let above = match cam.dist {
            None => cam.dir.z < -0.02,
            Some(_) => cam.eye().z > z0 + diag * 1e-4,
        };
        if above {
            ground_z = Some(z0);
            let eps = diag * 1e-5;
            {
                let (kind, depth) = (&g.kind, &t.depth);
                par_rows(&mut gdepth, w, |y0, rows| {
                    for (j, out) in rows.iter_mut().enumerate() {
                        let i = y0 * w + j;
                        let (sx, sy) = sample(i);
                        let Some(d) = cam.plane_depth(sx, sy, &plane) else { continue };
                        if cam.dist.is_some() && cam.ray(cam.unproject(sx, sy, d)).z >= 0.0 {
                            continue;
                        }
                        if kind[i] == NONE || d < depth[i] - eps {
                            *out = d;
                        }
                    }
                });
            }
            for (i, d) in gdepth.iter().enumerate() {
                if d.is_finite() {
                    g.kind[i] = GROUND;
                    g.albedo[i] = [st.ground_color[0], st.ground_color[1], st.ground_color[2]];
                    g.normal[i] = [0.0, 0.0, 1.0];
                }
            }
        }
    }
    let depth_of = |i: usize| if g.kind[i] == GROUND { gdepth[i] } else { t.depth[i] };

    // Ambient occlusion (screen space).
    let mut ao: Option<Vec<f32>> = None;
    let ao_radius = st.ao_radius.filter(|r| r.is_finite() && *r > 0.0).unwrap_or(diag * 0.05);
    if st.ao && bounds.is_some() {
        let lin: Vec<f64> =
            (0..w * h).map(|i| if g.kind[i] == NONE { f64::INFINITY } else { cam.linear_depth(depth_of(i)) }).collect();
        const K: usize = 16;
        let kernel: [DVec3; K] = std::array::from_fn(|k| {
            let u = (k as f64 + 0.5) / K as f64;
            let a = k as f64 * GOLDEN;
            let r = u.sqrt();
            let dir = DVec3::new(a.cos() * r, a.sin() * r, (1.0 - u).sqrt().max(0.15)).normalize();
            let s = ((k * 7) % K) as f64 / K as f64;
            dir * (0.15 + 0.85 * s * s)
        });
        let r = ao_radius;
        let mut raw = vec![1.0f32; w * h];
        {
            let (g, lin) = (&*g, &lin);
            par_rows(&mut raw, w, |y0, rows| {
                for (j, out) in rows.iter_mut().enumerate() {
                    let i = y0 * w + j;
                    if g.kind[i] == NONE {
                        continue;
                    }
                    let (x, y) = (i % w, i / w);
                    let (sx, sy) = sample(i);
                    let p = cam.unproject(sx, sy, depth_of(i));
                    let n = v3f(g.normal[i]);
                    let (b1, b2) = n.any_orthonormal_pair();
                    let a = BAYER4[(y % 4) * 4 + x % 4] / 16.0 * std::f64::consts::TAU;
                    let (ca, sa) = (a.cos(), a.sin());
                    let tx = b1 * ca + b2 * sa;
                    let ty = n.cross(tx);
                    let lp = lin[i];
                    let mut occ = 0.0;
                    for kv in &kernel {
                        let q = p + (tx * kv.x + ty * kv.y + n * kv.z) * r;
                        let (qx, qy, _) = cam.project(q);
                        if qx < 0.0 || qy < 0.0 || qx >= w as f64 || qy >= h as f64 {
                            continue;
                        }
                        let lq = (q - cam.target).dot(cam.dir);
                        let ls = lin[qy as usize * w + qx as usize];
                        if ls < lq - r * 0.03 {
                            let range = (lp - ls).abs();
                            occ += if range <= r { 1.0 } else { (r / range).powi(2) };
                        }
                    }
                    *out = (1.0 - occ / K as f64) as f32;
                }
            });
        }
        // depth/normal aware 4 × 4 blur (covers the rotation pattern)
        let mut blurred = vec![1.0f32; w * h];
        {
            let (g, lin, raw) = (&*g, &lin, &raw);
            par_rows(&mut blurred, w, |y0, rows| {
                for (j, out) in rows.iter_mut().enumerate() {
                    let i = y0 * w + j;
                    if g.kind[i] == NONE {
                        continue;
                    }
                    let (x, y) = (i % w, i / w);
                    let (mut s, mut n) = (0.0f32, 0.0f32);
                    for dy in -2i64..=1 {
                        for dx in -2i64..=1 {
                            let (xx, yy) = (x as i64 + dx, y as i64 + dy);
                            if xx < 0 || yy < 0 || xx >= w as i64 || yy >= h as i64 {
                                continue;
                            }
                            let k = yy as usize * w + xx as usize;
                            if g.kind[k] == NONE || (lin[k] - lin[i]).abs() > r * 0.25 {
                                continue;
                            }
                            let nd = g.normal[k][0] * g.normal[i][0]
                                + g.normal[k][1] * g.normal[i][1]
                                + g.normal[k][2] * g.normal[i][2];
                            if nd < 0.8 {
                                continue;
                            }
                            s += raw[k];
                            n += 1.0;
                        }
                    }
                    *out = if n > 0.0 { s / n } else { raw[i] };
                }
            });
        }
        let k = st.ao_strength.clamp(0.0, 2.0) as f32;
        for v in &mut blurred {
            *v = (1.0 - k * (1.0 - *v)).clamp(0.0, 1.0);
        }
        ao = Some(blurred);
    }

    // Shadow map along the key light.
    let sm_size = 2048usize;
    let sm = if st.shadows && light.z > -0.99 { ShadowMap::build(casters, light, sm_size) } else { None };
    let tan_theta = (st.shadow_softness.clamp(0.0, 1.0) * MAX_LIGHT_RADIUS_DEG).to_radians().tan();

    // Lighting.
    let fill = light_direction(cam.dir, cam.up, 0.0, (-cam.dir.z).asin().to_degrees() + 15.0);
    let amb = st.ambient.clamp(0.0, 1.0);
    let key_w = (1.0 - amb) * st.light_intensity.max(0.0);
    let fill_w = 0.22 * (1.0 - amb) * st.light_intensity.max(0.0);
    let gc = st.ground_color;
    let sky = [1.0f64, 1.0, 1.0];
    let bounce = [0.35 + 0.4 * gc[0] as f64, 0.35 + 0.4 * gc[1] as f64, 0.35 + 0.4 * gc[2] as f64];
    let fade = (diag * 0.55, diag * 1.5);
    let center_xy = DVec3::new(center.x, center.y, 0.0);
    let shadow_on_ground = |p: DVec3| -> f64 {
        // the ground's shadow also fades with the ground
        let r = (DVec3::new(p.x, p.y, 0.0) - center_xy).length();
        smoothstep(fade.0, fade.1, r)
    };
    {
        let (g, ao, sm) = (&*g, &ao, &sm);
        let depth = &t.depth;
        let gdepth = &gdepth;
        par_rows(&mut t.color, w, |y0, rows| {
            for (j, out) in rows.iter_mut().enumerate() {
                let i = y0 * w + j;
                let kind = g.kind[i];
                if kind == NONE {
                    continue;
                }
                let (sx, sy) = sample(i);
                let d = if kind == GROUND { gdepth[i] } else { depth[i] };
                let p = cam.unproject(sx, sy, d);
                let n = v3f(g.normal[i]);
                let view = -cam.ray(p);
                let ndl = n.dot(light);
                let vis = match sm {
                    Some(sm) if ndl > 0.0 => {
                        sm.visibility(p, n, ndl, tan_theta, ign(i % w, i / w) * std::f64::consts::TAU)
                    }
                    _ => 1.0,
                };
                let a = ao.as_ref().map_or(1.0, |v| v[i] as f64);
                let up = 0.5 + 0.5 * n.z;
                let key = key_w * ndl.max(0.0) * vis;
                let fl = fill_w * n.dot(fill).max(0.0);
                let direct_ao = 1.0 - 0.55 * (1.0 - a);
                let alb = g.albedo[i];
                let mut c = [0.0f32; 3];
                for k in 0..3 {
                    let ambient = amb * (bounce[k] + (sky[k] - bounce[k]) * up) * a;
                    c[k] = (alb[k] as f64 * (ambient + (key + fl) * direct_ao)) as f32;
                }
                if kind != GROUND && ndl > 0.0 {
                    let hv = (light + view).normalize_or_zero();
                    let spec = 0.28 * st.light_intensity.max(0.0) * vis * n.dot(hv).max(0.0).powf(48.0) * direct_ao;
                    for ck in &mut c {
                        *ck += spec as f32;
                    }
                }
                let c = c.map(tone);
                *out = if kind == GROUND {
                    let f = shadow_on_ground(p) as f32;
                    let bg = background_at(opts, i / w, h);
                    std::array::from_fn(|k| c[k] + (bg[k] - c[k]) * f)
                } else {
                    c
                };
            }
        });
    }
    StudioInfo {
        light_dir: light,
        ground_z,
        ao_radius: ao.as_ref().map(|_| ao_radius),
        shadow_map: sm.as_ref().map(|s| s.n as u32),
    }
}
