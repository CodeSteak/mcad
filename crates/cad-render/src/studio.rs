//! Studio look: deferred, physically based shading of the opaque samples after rasterisation.
//!
//! * Lights ([`Light`]): directional lights whose azimuth is relative to the camera (like a
//!   turntable: the model turns, the lights stay with the camera) and whose elevation is
//!   measured from the xy plane (z up). The first light casts the shadows.
//! * Materials ([`Material`]): metallic-roughness model per body (base colour from the model's
//!   colours): GGX microfacet distribution, height-correlated Smith visibility, Schlick
//!   Fresnel, Lambert diffuse; an optional clear coat (a second, smooth dielectric layer).
//! * Environment ([`Environment`]): reflections and ambient light of an analytic studio: a sky
//!   gradient, a ground bounce and softboxes, all spherical Gaussians (fixed relative to the
//!   camera like the lights). Prefiltered radiance for a roughness and diffuse irradiance are
//!   closed-form integrals of products of spherical Gaussians (no textures, no sampling); the
//!   split-sum environment BRDF uses Karis' analytic fit. Ambient occlusion also occludes the
//!   environment reflections.
//! * Shadows: an orthographic shadow map of the opaque (clipped) triangles along the first
//!   light, filtered with percentage-closer soft shadows (blocker search, penumbra from the
//!   distance between blocker and receiver, Vogel disk samples rotated per sample). The model
//!   shadows itself and the ground. Section caps and transparent items cast no shadow.
//! * Ambient occlusion: screen space, from the linear depth buffer: 16 samples in the normal
//!   hemisphere of radius `ao_radius` (mm, default 5 % of the model diagonal), rotated in a 4 × 4
//!   pattern and blurred over 4 × 4 samples with a depth/normal test.
//! * Ground: the plane z = lowest point of the model, only seen from above, faded into the
//!   background around the model. It does not hide overlays (grids, lines, labels).
//! * Shading in linear light, Khronos PBR Neutral tone mapping (keeps the model colours), sRGB
//!   output — the same as the UI (three.js `NeutralToneMapping`).
//! * Background: solid or vertical gradient ([`crate::RenderOptions::background_bottom`]).
//! * Mesh edge lines at reduced opacity (the shading shows the form), or none
//!   ([`Studio::edges`]).
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

/// A directional light of the studio look.
#[derive(Debug, Clone, PartialEq)]
pub struct Light {
    /// Azimuth in degrees relative to the camera: 0 = from the camera, positive = from the
    /// right, negative = from the left.
    pub azimuth: f64,
    /// Elevation in degrees above the xy plane (90 = from straight above).
    pub elevation: f64,
    /// Strength (1 = default key light).
    pub intensity: f64,
    /// Colour (sRGB, 0..1).
    pub color: [f32; 3],
}

impl Light {
    /// White light of strength 1 from `az`, `el`.
    pub fn new(azimuth: f64, elevation: f64) -> Light {
        Light { azimuth, elevation, intensity: 1.0, color: [1.0, 1.0, 1.0] }
    }

    /// The default key light: from the upper left (−40°, 50°).
    pub fn key() -> Light {
        Light::new(-40.0, 50.0)
    }
}

/// Surface of a body (metallic-roughness model; the base colour is the body's colour).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Material {
    /// 0 = dielectric (plastic), 1 = metal (the base colour tints the reflection).
    pub metalness: f32,
    /// Perceptual roughness: 0 = mirror … 1 = fully matte.
    pub roughness: f32,
    /// Strength of a smooth clear coat on top, 0..1.
    pub clearcoat: f32,
}

impl Default for Material {
    fn default() -> Self {
        Material::PLASTIC
    }
}

impl Material {
    /// Default of every body: matte plastic.
    pub const PLASTIC: Material = Material { metalness: 0.0, roughness: 0.5, clearcoat: 0.0 };

    /// Named presets (`--material SEL=PRESET`, `preset = "…"` in a `render` block).
    pub const PRESETS: [(&'static str, Material); 6] = [
        ("plastic", Material::PLASTIC),
        ("plastic_glossy", Material { metalness: 0.0, roughness: 0.2, clearcoat: 0.5 }),
        ("rubber", Material { metalness: 0.0, roughness: 0.9, clearcoat: 0.0 }),
        ("aluminium", Material { metalness: 1.0, roughness: 0.35, clearcoat: 0.0 }),
        ("metal_brushed", Material { metalness: 1.0, roughness: 0.55, clearcoat: 0.0 }),
        ("chrome", Material { metalness: 1.0, roughness: 0.08, clearcoat: 0.0 }),
    ];

    pub fn preset(name: &str) -> Option<Material> {
        let n = name.trim().to_ascii_lowercase();
        let n = if n == "aluminum" { "aluminium".to_string() } else { n };
        Material::PRESETS.iter().find(|(k, _)| *k == n).map(|(_, m)| *m)
    }

    /// Names of the presets, comma separated (messages).
    pub fn preset_names() -> String {
        Material::PRESETS.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", ")
    }

    /// Values clamped to their ranges.
    pub fn clamped(self) -> Material {
        Material {
            metalness: self.metalness.clamp(0.0, 1.0),
            roughness: self.roughness.clamp(0.0, 1.0),
            clearcoat: self.clearcoat.clamp(0.0, 1.0),
        }
    }
}

/// What the surroundings of the model look like in reflections and ambient light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Environment {
    /// Sky gradient, ground bounce and three softboxes (top, left, back right): crisp
    /// highlights on glossy and metal surfaces.
    #[default]
    Studio,
    /// Sky gradient and ground bounce only: soft, even light.
    Overcast,
    /// No reflections of the surroundings: hemisphere ambient light only (like the classic
    /// studio look).
    None,
}

impl Environment {
    pub const NAMES: [&'static str; 3] = ["studio", "overcast", "none"];

    pub fn from_name(s: &str) -> Option<Environment> {
        match s.trim().to_ascii_lowercase().as_str() {
            "studio" => Some(Environment::Studio),
            "overcast" => Some(Environment::Overcast),
            "none" => Some(Environment::None),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Environment::Studio => "studio",
            Environment::Overcast => "overcast",
            Environment::None => "none",
        }
    }
}

/// Settings of the studio look (defaults: [`Studio::default`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Studio {
    /// Directional lights; the first casts the shadows (empty: environment light only).
    pub lights: Vec<Light>,
    /// Factor on all lights (1 = default).
    pub light_intensity: f64,
    /// Share of the environment (sky) light, 0..1 (rest: the lights).
    pub ambient: f64,
    pub environment: Environment,
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
    /// Mesh edge lines of opaque bodies (at [`EDGE_ALPHA`] opacity); `false` hides them.
    pub edges: bool,
}

impl Default for Studio {
    fn default() -> Self {
        Studio {
            lights: vec![Light::key()],
            light_intensity: 1.0,
            ambient: 0.4,
            environment: Environment::Studio,
            shadows: true,
            shadow_softness: 0.35,
            ao: true,
            ao_radius: None,
            ao_strength: 1.0,
            ground: true,
            ground_color: [0.9, 0.905, 0.915, 1.0],
            edges: true,
        }
    }
}

impl Studio {
    /// The default settings with the key light at `az`, `el`.
    pub fn with_light(azimuth: f64, elevation: f64) -> Studio {
        Studio { lights: vec![Light::new(azimuth, elevation)], ..Studio::default() }
    }
}

/// What a studio rendering used.
#[derive(Debug, Clone, PartialEq)]
pub struct StudioInfo {
    /// Unit vector towards the first light (world; straight up without lights).
    pub light_dir: DVec3,
    /// Unit vectors towards all lights.
    pub light_dirs: Vec<DVec3>,
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

/// Factor of the environment reflections per unit of `ambient` (radiance as in
/// [`environment_spec`]). The diffuse environment light is normalised instead: a white surface
/// facing up receives `ambient` (as the hemisphere light of the classic studio look).
pub const ENV_SPECULAR_GAIN: f64 = 2.5;

/// A spherical Gaussian lobe of the environment: radiance `amp · exp(sharpness · (ω·dir − 1))`.
/// Direction as for lights: azimuth relative to the camera, elevation above the xy plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvLobe {
    pub azimuth: f64,
    pub elevation: f64,
    pub sharpness: f64,
    /// Linear RGB radiance at the centre.
    pub amp: [f64; 3],
}

/// The analytic environment: a constant radiance plus lobes (the UI builds its environment
/// map from the same numbers, `web/src/logic/studio.ts`).
#[derive(Debug, Clone, PartialEq)]
pub struct EnvSpec {
    pub constant: [f64; 3],
    pub lobes: Vec<EnvLobe>,
}

/// sRGB (0..1) → linear.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Linear → sRGB (0..1).
pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// [`linear_to_srgb`] as a table (4097 steps over 0..1, linear in between: < 0.2/255 off).
struct SrgbTable(Vec<f32>);

impl SrgbTable {
    const N: usize = 4096;

    fn new() -> SrgbTable {
        SrgbTable((0..=Self::N).map(|i| linear_to_srgb(i as f32 / Self::N as f32)).collect())
    }

    fn get(&self, v: f64) -> f32 {
        let x = v.clamp(0.0, 1.0) * Self::N as f64;
        let i = (x as usize).min(Self::N - 1);
        let f = (x - i as f64) as f32;
        self.0[i] + (self.0[i + 1] - self.0[i]) * f
    }
}

/// Khronos PBR Neutral tone mapping (three.js `NeutralToneMapping`): linear in, linear out
/// (0..1); colours below 0.76 stay, brighter ones are compressed towards white.
pub fn neutral_tone(c: [f64; 3]) -> [f64; 3] {
    const START: f64 = 0.8 - 0.04;
    const DESAT: f64 = 0.15;
    let mut c = c.map(|v| v.max(0.0));
    let x = c[0].min(c[1]).min(c[2]);
    let offset = if x < 0.08 { x - 6.25 * x * x } else { 0.04 };
    for v in &mut c {
        *v -= offset;
    }
    let peak = c[0].max(c[1]).max(c[2]);
    if peak < START {
        return c;
    }
    let d = 1.0 - START;
    let new_peak = 1.0 - d * d / (peak + d - START);
    let g = 1.0 - 1.0 / (DESAT * (peak - new_peak) + 1.0);
    c.map(|v| {
        let v = v * new_peak / peak;
        v + (new_peak - v) * g
    })
}

/// The environment of `env` (`ground`: linear colour of the ground bounce).
pub fn environment_spec(env: Environment, ground: [f64; 3]) -> EnvSpec {
    // a tall strip softbox: lobes stacked from below the horizon to 45° above
    let strip = |az: f64, a: f64| {
        [-30.0, -10.0, 10.0, 30.0, 50.0].map(|el| EnvLobe { azimuth: az, elevation: el, sharpness: 60.0, amp: [a; 3] })
    };
    let bounce = |k: f64| EnvLobe { azimuth: 0.0, elevation: -90.0, sharpness: 1.2, amp: ground.map(|g| g * k) };
    match env {
        Environment::Studio => EnvSpec {
            constant: [0.08, 0.08, 0.08],
            lobes: vec![
                EnvLobe { azimuth: 0.0, elevation: 90.0, sharpness: 2.0, amp: [0.50, 0.51, 0.54] },
                bounce(0.6),
                // softboxes: a large one above behind the model (seen in the top faces) and two
                // tall strips left and right (seen in the side faces)
                EnvLobe { azimuth: 180.0, elevation: 60.0, sharpness: 12.0, amp: [3.5, 3.5, 3.5] },
            ]
            .into_iter()
            .chain(strip(-70.0, 3.0))
            .chain(strip(115.0, 2.0))
            .collect(),
        },
        Environment::Overcast => EnvSpec {
            constant: [0.25, 0.25, 0.25],
            lobes: vec![
                EnvLobe { azimuth: 0.0, elevation: 90.0, sharpness: 0.8, amp: [0.74, 0.75, 0.78] },
                bounce(0.6),
            ],
        },
        Environment::None => EnvSpec { constant: [0.0; 3], lobes: Vec::new() },
    }
}

/// Irradiance of `spec` on a surface facing up, divided by π (1 for a uniform radiance 1).
pub fn up_irradiance(spec: &EnvSpec) -> f64 {
    let env = Env::new(spec, DVec3::new(0.0, 1.0, 0.0), DVec3::Z);
    let e = env.irradiance(DVec3::Z);
    (0.2126 * e[0] + 0.7152 * e[1] + 0.0722 * e[2]) / std::f64::consts::PI
}

/// `∫ exp(l1 (ω·d1 − 1)) exp(l2 (ω·d2 − 1)) dω` over the sphere (unit `d1`, `d2`).
fn sg_product(d1: DVec3, l1: f64, d2: DVec3, l2: f64) -> f64 {
    let m = (d1 * l1 + d2 * l2).length();
    if m < 1e-6 {
        return 4.0 * std::f64::consts::PI * (-l1 - l2).exp();
    }
    let e = m - l1 - l2;
    if e < -18.0 {
        return 0.0;
    }
    std::f64::consts::TAU * e.exp() * (1.0 - (-2.0 * m).exp()) / m
}

/// An [`EnvSpec`] placed for one camera: lobe directions in world space (linear radiance).
pub struct Env {
    constant: [f64; 3],
    lobes: Vec<(DVec3, f64, [f64; 3])>,
}

/// Sharpness and amplitude of the cosine lobe as a spherical Gaussian.
const COS_SHARP: f64 = 2.133;
const COS_AMP: f64 = 1.17;

impl Env {
    /// The environment for a camera with viewing direction `cam_dir` and screen up `cam_up`.
    pub fn new(spec: &EnvSpec, cam_dir: DVec3, cam_up: DVec3) -> Env {
        let lobes = spec
            .lobes
            .iter()
            .map(|l| (light_direction(cam_dir, cam_up, l.azimuth, l.elevation), l.sharpness, l.amp))
            .collect();
        Env { constant: spec.constant, lobes }
    }

    /// Radiance seen in direction `d`.
    pub fn radiance(&self, d: DVec3) -> [f64; 3] {
        let mut c = self.constant;
        for (dir, l, a) in &self.lobes {
            let k = (l * (d.dot(*dir) - 1.0)).exp();
            for i in 0..3 {
                c[i] += a[i] * k;
            }
        }
        c
    }

    /// Irradiance on a surface with normal `n` (`∫ L(ω) max(0, n·ω) dω`).
    pub fn irradiance(&self, n: DVec3) -> [f64; 3] {
        let mut c = self.constant.map(|v| v * std::f64::consts::PI);
        for (dir, l, a) in &self.lobes {
            let k = COS_AMP * sg_product(*dir, *l, n, COS_SHARP);
            for i in 0..3 {
                c[i] += a[i] * k;
            }
        }
        c
    }

    /// Radiance around the reflection direction `r`, averaged over the specular lobe of
    /// roughness `alpha` (GGX lobe as a normalised spherical Gaussian).
    pub fn prefiltered(&self, r: DVec3, alpha: f64) -> [f64; 3] {
        let lg = (0.5 / (alpha * alpha).max(1e-6)).clamp(0.25, 2e4);
        let norm = lg / (std::f64::consts::TAU * (1.0 - (-2.0 * lg).exp()));
        let mut c = self.constant;
        for (dir, l, a) in &self.lobes {
            let k = norm * sg_product(*dir, *l, r, lg);
            for i in 0..3 {
                c[i] += a[i] * k;
            }
        }
        c
    }
}

/// Analytic fit of the split-sum environment BRDF (Karis): `(scale, bias)` of F0.
fn env_brdf(n_dot_v: f64, roughness: f64) -> (f64, f64) {
    let c0 = [-1.0, -0.0275, -0.572, 0.022];
    let c1 = [1.0, 0.0425, 1.04, -0.04];
    let r: [f64; 4] = std::array::from_fn(|i| roughness * c0[i] + c1[i]);
    let a004 = (r[0] * r[0]).min((-9.28 * n_dot_v).exp2()) * r[0] + r[1];
    (-1.04 * a004 + r[2], 1.04 * a004 + r[3])
}

/// GGX distribution × height-correlated Smith visibility (`alpha` = roughness²).
fn ggx_dv(n_dot_h: f64, n_dot_l: f64, n_dot_v: f64, alpha: f64) -> f64 {
    let a2 = alpha * alpha;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    let dist = a2 / (std::f64::consts::PI * d * d);
    let gv = n_dot_l * (n_dot_v * n_dot_v * (1.0 - a2) + a2).sqrt();
    let gl = n_dot_v * (n_dot_l * n_dot_l * (1.0 - a2) + a2).sqrt();
    dist * 0.5 / (gv + gl).max(1e-9)
}

fn schlick(f0: f64, v_dot_h: f64) -> f64 {
    f0 + (1.0 - f0) * (1.0 - v_dot_h).clamp(0.0, 1.0).powi(5)
}

/// Smallest roughness (as three.js: avoids sub-pixel highlights).
const MIN_ROUGHNESS: f64 = 0.0525;
/// Roughness of the clear coat.
const CLEARCOAT_ROUGHNESS: f64 = 0.05;

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
    let lights: Vec<(DVec3, [f64; 3])> = st
        .lights
        .iter()
        .map(|l| {
            let dir = light_direction(cam.dir, cam.up, l.azimuth, l.elevation);
            let k = std::f64::consts::PI * l.intensity.max(0.0);
            (dir, l.color.map(|c| srgb_to_linear(c) as f64 * k))
        })
        .collect();
    let light = lights.first().map_or(DVec3::Z, |l| l.0);
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

    // Shadow map along the first light.
    let sm_size = 2048usize;
    let sm = if st.shadows && !lights.is_empty() && light.z > -0.99 {
        ShadowMap::build(casters, light, sm_size)
    } else {
        None
    };
    let tan_theta = (st.shadow_softness.clamp(0.0, 1.0) * MAX_LIGHT_RADIUS_DEG).to_radians().tan();

    // Lighting (linear light).
    let amb = st.ambient.clamp(0.0, 1.0);
    let light_w = (1.0 - amb) * st.light_intensity.max(0.0);
    let gc = st.ground_color;
    let ground_lin = [gc[0], gc[1], gc[2]].map(|c| srgb_to_linear(c) as f64);
    let spec_env = environment_spec(st.environment, ground_lin);
    let env = Env::new(&spec_env, cam.dir, cam.up);
    let env_spec_w = amb * ENV_SPECULAR_GAIN;
    let env_diff_w = amb / up_irradiance(&spec_env).max(1e-6);
    let reflections = st.environment != Environment::None;
    // hemisphere light of `Environment::None` (the classic studio look)
    let sky = [1.0f64, 1.0, 1.0];
    let bounce: [f64; 3] = std::array::from_fn(|k| 0.35 + 0.4 * ground_lin[k]);
    let fade = (diag * 0.55, diag * 1.5);
    let center_xy = DVec3::new(center.x, center.y, 0.0);
    let shadow_on_ground = |p: DVec3| -> f64 {
        // the ground's shadow also fades with the ground
        let r = (DVec3::new(p.x, p.y, 0.0) - center_xy).length();
        smoothstep(fade.0, fade.1, r)
    };
    let srgb = SrgbTable::new();
    {
        let (g, ao, sm) = (&*g, &ao, &sm);
        let depth = &t.depth;
        let gdepth = &gdepth;
        let (lights, env, srgb) = (&lights, &env, &srgb);
        par_rows(&mut t.color, w, |y0, rows| {
            // the albedo changes rarely along a row: convert it once per change
            let mut last_albedo = ([f32::NAN; 3], [0.0f64; 3]);
            // flat faces: the same normal (and in orthographic views the same reflection)
            let mut last_irr = ([f32::NAN; 3], [0.0f64; 3]);
            let mut last_pre = (DVec3::NAN, f64::NAN, [0.0f64; 3]);
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
                let n_dot_v = n.dot(view).clamp(1e-4, 1.0);
                let a = ao.as_ref().map_or(1.0, |v| v[i] as f64);
                let direct_ao = 1.0 - 0.55 * (1.0 - a);
                if g.albedo[i] != last_albedo.0 {
                    last_albedo = (g.albedo[i], g.albedo[i].map(|c| srgb_to_linear(c) as f64));
                }
                let albedo = last_albedo.1;
                // surface: ground and caps are matte dielectrics
                let m = if kind == SURFACE {
                    g.material[i]
                } else {
                    Material { metalness: 0.0, roughness: 1.0, clearcoat: 0.0 }
                };
                let metal = m.metalness as f64;
                let rough = (m.roughness as f64).max(MIN_ROUGHNESS);
                let alpha = rough * rough;
                let cc = if kind == SURFACE { m.clearcoat as f64 } else { 0.0 };
                let f0: [f64; 3] = std::array::from_fn(|k| 0.04 + (albedo[k] - 0.04) * metal);
                let diffuse: [f64; 3] = albedo.map(|c| c * (1.0 - metal));
                let mut c = [0.0f64; 3];
                let mut coat = [0.0f64; 3];
                // direct light
                for (li, (l, radiance)) in lights.iter().enumerate() {
                    let n_dot_l = n.dot(*l);
                    if n_dot_l <= 0.0 {
                        continue;
                    }
                    let vis = match sm {
                        Some(sm) if li == 0 => {
                            sm.visibility(p, n, n_dot_l, tan_theta, ign(i % w, i / w) * std::f64::consts::TAU)
                        }
                        _ => 1.0,
                    };
                    if vis <= 0.0 {
                        continue;
                    }
                    let hv = (*l + view).normalize_or_zero();
                    let (n_dot_h, v_dot_h) = (n.dot(hv).max(0.0), view.dot(hv).max(0.0));
                    let e = light_w * n_dot_l * vis * direct_ao;
                    let dv = if kind == GROUND { 0.0 } else { ggx_dv(n_dot_h, n_dot_l, n_dot_v, alpha) };
                    for k in 0..3 {
                        let spec = dv * schlick(f0[k], v_dot_h);
                        c[k] += (diffuse[k] / std::f64::consts::PI + spec) * radiance[k] * e;
                    }
                    if cc > 0.0 {
                        let dc = ggx_dv(n_dot_h, n_dot_l, n_dot_v, CLEARCOAT_ROUGHNESS.max(MIN_ROUGHNESS).powi(2));
                        let f = schlick(0.04, v_dot_h);
                        for k in 0..3 {
                            coat[k] += dc * f * radiance[k] * e;
                        }
                    }
                }
                // environment: diffuse irradiance and prefiltered reflections
                if reflections {
                    if g.normal[i] != last_irr.0 {
                        last_irr = (g.normal[i], env.irradiance(n));
                    }
                    let irr = last_irr.1;
                    let r = (n * (2.0 * n.dot(view)) - view).normalize_or_zero();
                    let (sa, sb) = env_brdf(n_dot_v, rough);
                    let pre = if kind == GROUND {
                        [0.0; 3]
                    } else {
                        if r != last_pre.0 || alpha != last_pre.1 {
                            last_pre = (r, alpha, env.prefiltered(r, alpha));
                        }
                        last_pre.2
                    };
                    for k in 0..3 {
                        let fss = f0[k] * sa + sb;
                        let dif = diffuse[k] / std::f64::consts::PI * irr[k] * (1.0 - fss) * env_diff_w;
                        c[k] += (dif + pre[k] * fss * env_spec_w) * a;
                    }
                    if cc > 0.0 {
                        let (ca, cb) = env_brdf(n_dot_v, CLEARCOAT_ROUGHNESS.max(MIN_ROUGHNESS));
                        let pc = env.prefiltered(r, CLEARCOAT_ROUGHNESS.max(MIN_ROUGHNESS).powi(2));
                        for k in 0..3 {
                            coat[k] += pc[k] * (0.04 * ca + cb) * env_spec_w * a;
                        }
                    }
                } else {
                    let up = 0.5 + 0.5 * n.z;
                    for k in 0..3 {
                        c[k] += diffuse[k] * amb * (bounce[k] + (sky[k] - bounce[k]) * up) * a;
                    }
                }
                if cc > 0.0 {
                    let fc = schlick(0.04, n_dot_v);
                    for k in 0..3 {
                        c[k] = c[k] * (1.0 - cc * fc) + cc * coat[k];
                    }
                }
                let lin = neutral_tone(c);
                let c: [f32; 3] = lin.map(|v| srgb.get(v));
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
        light_dirs: lights.iter().map(|l| l.0).collect(),
        ground_z,
        ao_radius: ao.as_ref().map(|_| ao_radius),
        shadow_map: sm.as_ref().map(|s| s.n as u32),
    }
}
