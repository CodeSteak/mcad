//! Studio look: deferred, physically based shading of the opaque samples after rasterisation.
//!
//! * Lights ([`Light`]): directional lights whose azimuth is relative to the camera (like a
//!   turntable: the model turns, the lights stay with the camera) and whose elevation is
//!   measured from the xy plane (z up). The first light casts the shadows.
//! * Materials ([`Material`]): metallic-roughness model per body (base colour from the model's
//!   colours, or the preset's own, e.g. the reflectance of gold): GGX microfacet
//!   distribution (anisotropic for brushed metal), height-correlated Smith visibility, Schlick
//!   Fresnel, Lambert diffuse, multiple-scattering energy compensation (rough metals keep
//!   their brightness); an optional clear coat (a second, smooth dielectric layer). The same
//!   model as three.js `MeshPhysicalMaterial` in the UI.
//! * Environment ([`Environment`], [`crate::env`]): reflections and ambient light of a
//!   procedural high dynamic range studio (dark walls, lit floor, rectangular softboxes),
//!   fixed relative to the camera like the lights, prefiltered for the roughness (split sum
//!   with a numerically computed DFG table). Ambient occlusion also occludes the environment
//!   reflections.
//! * Reflections of the scene ([`Studio::reflections`]): rays from glossy surfaces (metals with
//!   roughness below 0.6, dielectrics below 0.35; a few per sample by [`Quality`]) are traced
//!   through a bounding volume hierarchy of the opaque triangles: the model reflects itself
//!   and the ground with its shadow; several rays per sample are smoothed over neighbouring
//!   samples (Gaussian, normal and plane aware), clamped against fireflies. The ground mirrors the model ([`Studio::ground_reflection`], blurred by
//!   [`Studio::ground_roughness`], fading with the height above the ground).
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
//! No textures and no image files: the environment is generated in code. Deterministic (the
//! sample rotations are fixed patterns), parallel over image rows.

use glam::DVec3;

use crate::env::{Dfg, camera_azimuth, smith_vis};
pub use crate::env::{ENV_SPECULAR_GAIN, EnvFn, EnvMaps, EnvSpec, Softbox, environment_spec, up_irradiance};
use crate::rt::{Bvh, RtTri};
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

/// Surface of a body (metallic-roughness model; the base colour is the body's colour unless
/// the material has its own, like the real metals).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Material {
    /// 0 = dielectric (plastic), 1 = metal (the base colour tints the reflection).
    pub metalness: f32,
    /// Perceptual roughness: 0 = mirror … 1 = fully matte.
    pub roughness: f32,
    /// Strength of a smooth clear coat on top, 0..1.
    pub clearcoat: f32,
    /// Brushed finish, 0..1: highlights stretch across the brushing (anisotropic GGX). The
    /// brushing runs around the body's z axis (lathe turned look): around the walls and in
    /// circles on faces across the axis.
    pub anisotropy: f32,
    /// Base colour (linear RGB) instead of the body's colour, e.g. the reflectance of a metal.
    pub color: Option<[f32; 3]>,
}

impl Default for Material {
    fn default() -> Self {
        Material::PLASTIC
    }
}

const fn mat(metalness: f32, roughness: f32, clearcoat: f32) -> Material {
    Material { metalness, roughness, clearcoat, anisotropy: 0.0, color: None }
}

const fn metal(color: [f32; 3], roughness: f32) -> Material {
    Material { metalness: 1.0, roughness, clearcoat: 0.0, anisotropy: 0.0, color: Some(color) }
}

/// Reflectance (F0, linear RGB) of real metals.
pub mod f0 {
    pub const ALUMINIUM: [f32; 3] = [0.913, 0.922, 0.924];
    pub const CHROME: [f32; 3] = [0.55, 0.556, 0.554];
    pub const STAINLESS: [f32; 3] = [0.65, 0.64, 0.62];
    pub const TITANIUM: [f32; 3] = [0.542, 0.497, 0.449];
    pub const GOLD: [f32; 3] = [1.0, 0.766, 0.336];
    pub const COPPER: [f32; 3] = [0.955, 0.638, 0.538];
    pub const BRASS: [f32; 3] = [0.91, 0.778, 0.423];
}

impl Material {
    /// Default of every body: matte plastic.
    pub const PLASTIC: Material = mat(0.0, 0.45, 0.0);

    /// Named presets (`--material SEL=PRESET`, `preset = "…"` in a `render` block; the UI has
    /// the same table: `web/src/logic/studio.ts`).
    pub const PRESETS: [(&'static str, Material); 14] = [
        ("plastic", Material::PLASTIC),
        ("plastic_satin", mat(0.0, 0.3, 0.0)),
        ("plastic_glossy", mat(0.0, 0.12, 0.6)),
        ("rubber", mat(0.0, 0.85, 0.0)),
        ("ceramic", mat(0.0, 0.35, 1.0)),
        ("aluminium", metal(f0::ALUMINIUM, 0.2)),
        ("metal_brushed", Material { anisotropy: 0.8, ..metal(f0::ALUMINIUM, 0.22) }),
        ("anodized", mat(1.0, 0.28, 0.0)),
        ("chrome", metal(f0::CHROME, 0.04)),
        ("stainless", metal(f0::STAINLESS, 0.12)),
        ("titanium", metal(f0::TITANIUM, 0.25)),
        ("gold", metal(f0::GOLD, 0.12)),
        ("copper", metal(f0::COPPER, 0.18)),
        ("brass", metal(f0::BRASS, 0.15)),
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
            anisotropy: self.anisotropy.clamp(0.0, 1.0),
            color: self.color.map(|c| c.map(|v| v.clamp(0.0, 1.0))),
        }
    }
}

/// What the surroundings of the model look like in reflections and ambient light
/// ([`crate::env::environment_spec`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Environment {
    /// Dark studio with bright softboxes (top, key strip left, fill strip right): contrasty
    /// reflections, metals look like metal.
    #[default]
    Studio,
    /// Light grey studio with large soft boxes: gentle reflections, low contrast.
    Soft,
    /// Sky gradient only: soft, even light.
    Overcast,
    /// No reflections of the surroundings: hemisphere ambient light only (like the classic
    /// studio look).
    None,
}

impl Environment {
    pub const NAMES: [&'static str; 4] = ["studio", "soft", "overcast", "none"];

    pub fn from_name(s: &str) -> Option<Environment> {
        match s.trim().to_ascii_lowercase().as_str() {
            "studio" => Some(Environment::Studio),
            "soft" => Some(Environment::Soft),
            "overcast" => Some(Environment::Overcast),
            "none" => Some(Environment::None),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Environment::Studio => "studio",
            Environment::Soft => "soft",
            Environment::Overcast => "overcast",
            Environment::None => "none",
        }
    }
}

/// Ray tracing effort of the studio look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// No ray tracing: reflections of the environment only, no reflections of the model.
    Draft,
    /// Ray-traced reflections with few rays per sample.
    #[default]
    Normal,
    /// More rays per sample: smoother glossy reflections (slower).
    High,
}

impl Quality {
    pub const NAMES: [&'static str; 3] = ["draft", "normal", "high"];

    pub fn from_name(s: &str) -> Option<Quality> {
        match s.trim().to_ascii_lowercase().as_str() {
            "draft" => Some(Quality::Draft),
            "normal" => Some(Quality::Normal),
            "high" => Some(Quality::High),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Quality::Draft => "draft",
            Quality::Normal => "normal",
            Quality::High => "high",
        }
    }

    /// Rays per sample for glossy surfaces and for the ground.
    fn rays(self) -> (u32, u32) {
        match self {
            Quality::Draft => (0, 0),
            Quality::Normal => (4, 4),
            Quality::High => (8, 8),
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
    /// Mirror image of the model in the ground, 0 = none … 1 = strong (glossy floor).
    pub ground_reflection: f64,
    /// Blur of the ground reflection, 0 = sharp mirror … 1 = very blurred.
    pub ground_roughness: f64,
    /// Ray-traced reflections of the model in itself and of the ground with the model's
    /// shadow (`false`: the environment only).
    pub reflections: bool,
    pub quality: Quality,
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
            ground_reflection: 0.2,
            ground_roughness: 0.3,
            reflections: true,
            quality: Quality::Normal,
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
    /// Ray-traced reflection rays per sample: glossy surfaces, ground (`None`: no ray
    /// tracing).
    pub rays: Option<(u32, u32)>,
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
    fn build(casters: &[RtTri], light: DVec3, n: usize) -> Option<ShadowMap> {
        if casters.is_empty() {
            return None;
        }
        let lz = -light;
        let lx = if lz.z.abs() < 0.9 { DVec3::Z.cross(lz) } else { DVec3::X.cross(lz) }.normalize();
        let ly = lz.cross(lx);
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for t in casters {
            for p in &t.p {
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
            let q: [(f64, f64, f64); 3] = std::array::from_fn(|k| sm.coords(t.p[k]));
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

    /// Lit fraction with 4 taps around the point (reflections: cheap, slightly soft).
    fn visibility_fast(&self, p: DVec3, n: DVec3, ndl: f64) -> f64 {
        let p = p + n * self.texel * (1.0 + 1.5 * (1.0 - ndl.clamp(0.0, 1.0)));
        let (x, y, d) = self.coords(p);
        let bias = self.texel * 3.0;
        let lit = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
            .iter()
            .filter(|(ox, oy)| self.at(x + ox, y + oy) as f64 >= d - bias)
            .count();
        lit as f64 / 4.0
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

/// Smallest roughness (as three.js: avoids sub-pixel highlights).
const MIN_ROUGHNESS: f64 = 0.0525;
/// Roughness of the clear coat.
const CLEARCOAT_ROUGHNESS: f64 = 0.05;
/// Rougher surfaces reflect the environment only (their reflections of the scene would be
/// too blurred to show and too noisy with few rays).
const RT_MAX_ROUGHNESS: f64 = 0.6;
/// The same for dielectrics (plastics): their reflections of the scene are faint.
const RT_MAX_ROUGHNESS_DIELECTRIC: f64 = 0.35;

/// What the shading needs to know about a scene item.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ItemInfo {
    pub material: Material,
    /// Linear base colour of the item (its colour, or the material's own).
    pub albedo: [f64; 3],
    /// The item's z axis and origin in the world (brushing of anisotropic materials).
    pub axis: DVec3,
    pub origin: DVec3,
}

impl ItemInfo {
    pub fn new(material: Material, color: [f32; 4], transform: &glam::DMat4) -> ItemInfo {
        let material = material.clamped();
        let albedo = match material.color {
            Some(c) => c.map(|v| v as f64),
            None => [color[0], color[1], color[2]].map(|v| srgb_to_linear(v) as f64),
        };
        ItemInfo {
            material,
            albedo,
            axis: transform.transform_vector3(DVec3::Z).normalize_or(DVec3::Z),
            origin: transform.transform_point3(DVec3::ZERO),
        }
    }
}

/// Direction in which the highlights of a brushed surface stretch (unit, in the tangent plane
/// of `n`): across the brushing, which runs around `axis` through `origin` — on walls around
/// the axis, on faces across the axis in circles (a lathe-turned or spun finish). The UI uses
/// the same rule (`brushFrame` in `web/src/view/materials.ts`).
pub fn brush_stretch(n: DVec3, p: DVec3, axis: DVec3, origin: DVec3) -> DVec3 {
    let r = p - origin;
    let radial = (r - axis * r.dot(axis)).normalize_or_zero();
    let groove = axis.cross(n) + axis.cross(radial) * 0.25;
    let groove = groove - n * groove.dot(n);
    let groove = if groove.length_squared() < 1e-10 { n.any_orthonormal_vector() } else { groove.normalize() };
    n.cross(groove)
}

/// Widening of the specular lobe for the lights: they act like round area lights of about
/// 10° radius (Karis' sphere light approximation: `alpha + tan(radius) / 2`, energy kept),
/// so smooth metals show soft discs instead of pinpoint highlights. The UI patches three.js
/// the same way (`LIGHT_SPREAD` in `web/src/view/materials.ts`).
pub const LIGHT_SPREAD: f64 = 0.088;

/// GGX distribution of the lights (widened by [`LIGHT_SPREAD`]) × height-correlated Smith
/// visibility (`alpha` = roughness²).
fn ggx_dv(n_dot_h: f64, n_dot_l: f64, n_dot_v: f64, alpha: f64) -> f64 {
    let aw = (alpha + LIGHT_SPREAD).min(1.0);
    let a2 = aw * aw;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    let dist = a2 / (std::f64::consts::PI * d * d) * (alpha / aw).powi(2);
    dist * smith_vis(n_dot_l, n_dot_v, alpha)
}

/// Anisotropic GGX (widened like [`ggx_dv`]) × Smith visibility (Filament, as three.js):
/// roughness `at` along the stretch direction `t`, `ab` along `b`.
#[allow(clippy::too_many_arguments)]
fn ggx_dv_aniso(at: f64, ab: f64, t: DVec3, b: DVec3, n: DVec3, l: DVec3, v: DVec3, h: DVec3) -> f64 {
    let (n_dot_l, n_dot_v, n_dot_h) = (n.dot(l), n.dot(v), n.dot(h));
    let (atw, abw) = ((at + LIGHT_SPREAD).min(1.0), (ab + LIGHT_SPREAD).min(1.0));
    let a2 = atw * abw;
    let q = DVec3::new(abw * t.dot(h), atw * b.dot(h), a2 * n_dot_h);
    let w2 = a2 / q.length_squared().max(1e-30);
    let d = a2 * w2 * w2 / std::f64::consts::PI * (at * ab / a2);
    let gv = n_dot_l * DVec3::new(at * t.dot(v), ab * b.dot(v), n_dot_v).length();
    let gl = n_dot_v * DVec3::new(at * t.dot(l), ab * b.dot(l), n_dot_l).length();
    d * 0.5 / (gv + gl).max(1e-12)
}

fn schlick(f0: f64, v_dot_h: f64) -> f64 {
    f0 + (1.0 - f0) * (1.0 - v_dot_h).clamp(0.0, 1.0).powi(5)
}

/// Half vector of the GGX distribution for `(u1, u2)` around `n` (frame `t1`, `t2`).
fn ggx_half(u1: f64, u2: f64, alpha: f64, n: DVec3, t1: DVec3, t2: DVec3) -> DVec3 {
    let a2 = alpha * alpha;
    let cos_t = ((1.0 - u1) / (1.0 + (a2 - 1.0) * u1)).max(0.0).sqrt();
    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
    let phi = std::f64::consts::TAU * u2;
    t1 * (sin_t * phi.cos()) + t2 * (sin_t * phi.sin()) + n * cos_t
}

/// One surface point to shade.
struct Surf {
    p: DVec3,
    /// Unit normal facing the viewer.
    n: DVec3,
    /// Unit vector towards the viewer.
    view: DVec3,
    /// Linear base colour.
    base: [f64; 3],
    m: Material,
    /// Stretch direction of a brushed material.
    stretch: Option<DVec3>,
    ao: f64,
    /// Rotation of the shadow filter pattern (`None`: a cheap 4-tap shadow, for reflections).
    phi: Option<f64>,
    /// The ground: matte, no specular reflection.
    ground: bool,
}

/// Lights, shadows and environment of one rendering.
struct Ctx<'a> {
    lights: &'a [(DVec3, [f64; 3])],
    light_w: f64,
    sm: Option<&'a ShadowMap>,
    tan_theta: f64,
    maps: Option<&'a EnvMaps>,
    /// cos, sin of the rotation world → environment (camera at azimuth 0).
    rot: (f64, f64),
    env_spec_w: f64,
    env_diff_w: f64,
    /// Environment `none`: hemisphere light (ambient, ground bounce).
    amb: f64,
    bounce: [f64; 3],
    dfg: &'static Dfg,
    items: &'a [ItemInfo],
    tracer: Option<&'a Tracer<'a>>,
}

impl Ctx<'_> {
    fn to_env(&self, d: DVec3) -> DVec3 {
        let (c, s) = self.rot;
        DVec3::new(d.x * c - d.y * s, d.x * s + d.y * c, d.z)
    }

    /// Environment radiance as reflections see it (roughness 0 = mirror).
    fn env_reflection(&self, d: DVec3, rough: f64) -> [f64; 3] {
        match self.maps {
            Some(m) => m.specular(self.to_env(d), rough).map(|v| v * self.env_spec_w),
            None => [0.0; 3],
        }
    }

    /// Linear outgoing radiance of `s`; `trace` = (sample offset, rotation, rays of glossy
    /// surfaces wanted) traces the reflections of the scene. Second: the part of glossy
    /// (several rays) reflections that differs from the environment (noisy, the caller
    /// smooths it; `Some([NAN; 3])`: glossy, but no rays traced for this sample).
    fn shade(&self, s: &Surf, trace: Option<(f64, f64, bool)>) -> ([f64; 3], Option<[f64; 3]>) {
        let (n, view, m) = (s.n, s.view, &s.m);
        let n_dot_v = n.dot(view).clamp(1e-4, 1.0);
        let metal = m.metalness as f64;
        let rough = (m.roughness as f64).max(MIN_ROUGHNESS);
        let alpha = rough * rough;
        let base = s.base;
        let f0: [f64; 3] = std::array::from_fn(|k| 0.04 + (base[k] - 0.04) * metal);
        let diffuse: [f64; 3] = base.map(|c| c * (1.0 - metal));
        let (da, db) = self.dfg.lookup(rough, n_dot_v);
        let ess = (da + db).clamp(1e-3, 1.0);
        let an = m.anisotropy as f64;
        // brushed: stretch direction t (roughness rises along it), b across it
        let aniso = s.stretch.filter(|_| an > 0.0 && !s.ground).map(|t| {
            let at = alpha + (1.0 - alpha) * an * an;
            (t, n.cross(t), at)
        });
        let cc = if s.ground { 0.0 } else { m.clearcoat as f64 };
        let direct_ao = 1.0 - 0.55 * (1.0 - s.ao);
        let mut c = [0.0f64; 3];
        let mut coat = [0.0f64; 3];
        let mut refl: Option<[f64; 3]> = None;
        for (li, (l, radiance)) in self.lights.iter().enumerate() {
            let n_dot_l = n.dot(*l);
            if n_dot_l <= 0.0 {
                continue;
            }
            let vis = match (self.sm, s.phi) {
                (Some(sm), Some(phi)) if li == 0 => sm.visibility(s.p, n, n_dot_l, self.tan_theta, phi),
                (Some(sm), None) if li == 0 => sm.visibility_fast(s.p, n, n_dot_l),
                _ => 1.0,
            };
            if vis <= 0.0 {
                continue;
            }
            let h = (*l + view).normalize_or_zero();
            let (n_dot_h, v_dot_h) = (n.dot(h).max(0.0), view.dot(h).max(0.0));
            let e = self.light_w * n_dot_l * vis * direct_ao;
            let dv = if s.ground {
                0.0
            } else {
                match aniso {
                    Some((t, b, at)) => ggx_dv_aniso(at, alpha, t, b, n, *l, view, h),
                    None => ggx_dv(n_dot_h, n_dot_l, n_dot_v, alpha),
                }
            };
            let fc = (1.0 - v_dot_h).clamp(0.0, 1.0).powi(5);
            for k in 0..3 {
                let f = f0[k] + (1.0 - f0[k]) * fc;
                // multiple-scattering compensation (Turquin), as three.js
                let comp = 1.0 + f0[k] * (1.0 / ess - 1.0);
                c[k] += (diffuse[k] / std::f64::consts::PI * (1.0 - f) + dv * f * comp) * radiance[k] * e;
            }
            if cc > 0.0 {
                let dc = ggx_dv(n_dot_h, n_dot_l, n_dot_v, CLEARCOAT_ROUGHNESS.max(MIN_ROUGHNESS).powi(2));
                let f = schlick(0.04, v_dot_h);
                for k in 0..3 {
                    coat[k] += dc * f * radiance[k] * e;
                }
            }
        }
        match self.maps {
            Some(maps) => {
                let irr = maps.irradiance(self.to_env(n));
                // reflection direction; brushed: around the bent normal (Filament)
                let nb = match aniso {
                    Some((_, b, _)) => {
                        let bn = b.cross(view).cross(b).normalize_or(n);
                        let k = (1.0 - an * (1.0 - rough)).powi(4);
                        (bn + (n - bn) * k).normalize_or(n)
                    }
                    None => n,
                };
                let r = (nb * (2.0 * nb.dot(view)) - view).normalize_or(n);
                // brushed: as rough as the mean of both directions (the rays sample a round lobe)
                let rt_rough = aniso.map_or(rough, |(_, _, at)| (at * alpha).sqrt().sqrt());
                // dielectrics reflect a few percent: rays only where the reflection is sharp
                let rt_max = if metal >= 0.5 { RT_MAX_ROUGHNESS } else { RT_MAX_ROUGHNESS_DIELECTRIC };
                let mut spec_ao = s.ao;
                let mut corr = None;
                let pre = if s.ground {
                    [0.0; 3]
                } else {
                    let env = self.env_reflection(r, rough);
                    match (self.tracer, trace) {
                        (Some(tr), Some((u, phi, take))) if rt_rough < rt_max => {
                            match tr.glossy(self, s.p, n, view, r, alpha, u, phi, take) {
                                // one mirror ray: exact, no noise
                                Glossy::Mirror(c) => {
                                    spec_ao = 1.0;
                                    std::array::from_fn(|k| (env[k] * s.ao + c[k]).max(0.0))
                                }
                                Glossy::Rays(c) => {
                                    corr = Some(c);
                                    env
                                }
                                Glossy::Skipped => {
                                    corr = Some([f64::NAN; 3]);
                                    env
                                }
                                Glossy::None => env,
                            }
                        }
                        _ => env,
                    }
                };
                let ems = 1.0 - ess;
                let fms = |fss: f64, fr: f64| {
                    let favg = fr + (1.0 - fr) / 21.0;
                    fss * favg / (1.0 - ems * favg) * ems
                };
                for k in 0..3 {
                    let (fss_d, fss_m) = (0.04 * da + db, base[k] * da + db);
                    let (ms_d, ms_m) = (fms(fss_d, 0.04), fms(fss_m, base[k]));
                    let single = fss_d + (fss_m - fss_d) * metal;
                    let multi = ms_d + (ms_m - ms_d) * metal;
                    let spec = if s.ground {
                        0.0
                    } else {
                        pre[k] * single * spec_ao + multi * irr[k] / std::f64::consts::PI * self.env_spec_w * s.ao
                    };
                    if let Some(cr) = corr {
                        // the ambient occlusion of the environment part stays
                        refl.get_or_insert([0.0; 3])[k] = cr[k] * single;
                    }
                    let dif =
                        diffuse[k] * (1.0 - fss_d - ms_d) * irr[k] / std::f64::consts::PI * self.env_diff_w * s.ao;
                    c[k] += spec + dif;
                }
                if cc > 0.0 {
                    let cr = CLEARCOAT_ROUGHNESS.max(MIN_ROUGHNESS);
                    let (ca, cb) = self.dfg.lookup(cr, n_dot_v);
                    let rc = (n * (2.0 * n.dot(view)) - view).normalize_or(n);
                    let pc = self.env_reflection(rc, cr);
                    for k in 0..3 {
                        coat[k] += pc[k] * (0.04 * ca + cb) * s.ao;
                    }
                }
            }
            None => {
                let up = 0.5 + 0.5 * n.z;
                for k in 0..3 {
                    c[k] += diffuse[k] * self.amb * (self.bounce[k] + (1.0 - self.bounce[k]) * up) * s.ao;
                }
            }
        }
        if cc > 0.0 {
            let fc = schlick(0.04, n_dot_v);
            for k in 0..3 {
                c[k] = c[k] * (1.0 - cc * fc) + cc * coat[k];
                if let Some(r) = refl.as_mut() {
                    r[k] *= 1.0 - cc * fc;
                }
            }
        }
        (c, refl)
    }
}

/// Largest radiance of one of several glossy rays (fireflies).
const RAY_CLAMP: f64 = 1.5;

/// Roughness of the environment lookups of reflection rays (the floor in reflections).
const GROUND_ENV_ROUGHNESS: f64 = 0.15;

/// The ground as reflections see it.
struct GroundInfo {
    z: f64,
    center: DVec3,
    /// Radii where it starts and ends fading into the environment.
    fade: (f64, f64),
}

/// Result of [`Tracer::glossy`].
enum Glossy {
    /// No rays (none wanted for this roughness).
    None,
    /// One mirror ray: the correction of the environment.
    Mirror([f64; 3]),
    /// Several rays: the (noisy) correction of the environment.
    Rays([f64; 3]),
    /// Glossy, but this sample traces no rays (its neighbours do).
    Skipped,
}

/// Rays through the scene for the reflections.
struct Tracer<'a> {
    bvh: &'a Bvh,
    tris: &'a [RtTri],
    /// Offset of ray origins from surfaces (mm).
    eps: f64,
    /// Rays per sample: glossy surfaces, ground.
    rays: (u32, u32),
    /// Glossy and ground rays only from every second sample in x and y.
    sparse: bool,
    ground: Option<GroundInfo>,
    /// Share of the ground's light that the key light's shadow takes away.
    shadow_share: f64,
}

impl Tracer<'_> {
    /// Reflection of the scene around `r` for a surface of roughness `alpha` (squared), as a
    /// correction of the prefiltered environment: the mean of (traced radiance − environment
    /// radiance) over the rays (a control variate: where a ray sees what the environment
    /// shows, e.g. the unshadowed floor, it adds no noise). Several rays only if `take`.
    #[allow(clippy::too_many_arguments)]
    fn glossy(
        &self,
        ctx: &Ctx,
        p: DVec3,
        n: DVec3,
        view: DVec3,
        r: DVec3,
        alpha: f64,
        u: f64,
        phi: f64,
        take: bool,
    ) -> Glossy {
        let k = if alpha < 0.0064 { 1 } else { self.rays.0 };
        if k == 0 {
            return Glossy::None;
        }
        if k > 1 && !take {
            return Glossy::Skipped;
        }
        let o = p + n * self.eps;
        let (t1, t2) = n.any_orthonormal_pair();
        let mut sum = [0.0f64; 3];
        for i in 0..k {
            let d = if k == 1 {
                r
            } else {
                let u1 = ((i as f64 + u) / k as f64).fract();
                let u2 = (i as f64 * 0.618_034 + phi).fract();
                let h = ggx_half(u1, u2, alpha, n, t1, t2);
                let l = h * (2.0 * view.dot(h)) - view;
                if l.dot(n) <= 1e-3 {
                    continue;
                }
                l.normalize()
            };
            if let Some(c) = self.radiance(ctx, o, d) {
                let e = ctx.env_reflection(d, GROUND_ENV_ROUGHNESS);
                for q in 0..3 {
                    // several rays: clamped against fireflies (bright highlights hit by a ray)
                    let c = if k > 1 { c[q].min(RAY_CLAMP) } else { c[q] };
                    sum[q] += c - e[q].min(if k > 1 { RAY_CLAMP } else { f64::INFINITY });
                }
            }
        }
        let c = sum.map(|v| v / k as f64);
        if k > 1 { Glossy::Rays(c) } else { Glossy::Mirror(c) }
    }

    /// Radiance along the ray `o + t d`: the model or the ground with the model's shadow
    /// (`None`: the environment).
    fn radiance(&self, ctx: &Ctx, o: DVec3, d: DVec3) -> Option<[f64; 3]> {
        let tg = self.ground.as_ref().and_then(|g| (d.z < -1e-6 && o.z > g.z).then(|| (g.z - o.z) / d.z));
        if let Some(h) = self.bvh.intersect(o, d, 0.0, tg.unwrap_or(f64::INFINITY)) {
            return Some(self.shade_hit(ctx, h.tri as usize, h.u, h.v, d));
        }
        let (g, t) = (self.ground.as_ref()?, tg?);
        let q = o + d * t;
        let fade = smoothstep(g.fade.0, g.fade.1, (DVec3::new(q.x, q.y, 0.0) - g.center).length());
        if fade >= 1.0 {
            return None;
        }
        let env = ctx.env_reflection(d, GROUND_ENV_ROUGHNESS);
        let vis = match (ctx.sm, ctx.lights.first()) {
            (Some(sm), Some((l, _))) if l.z > 0.0 => sm.visibility_fast(q, DVec3::Z, l.z),
            _ => 1.0,
        };
        let k = 1.0 - self.shadow_share * (1.0 - vis) * (1.0 - fade);
        Some(env.map(|v| v * k))
    }

    /// Shaded hit on triangle `tri` (barycentric `u`, `v`) seen along `d`.
    fn shade_hit(&self, ctx: &Ctx, tri: usize, u: f64, v: f64, d: DVec3) -> [f64; 3] {
        let t = &self.tris[tri];
        let w0 = 1.0 - u - v;
        let p = t.p[0] * w0 + t.p[1] * u + t.p[2] * v;
        let mut ng = (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).normalize_or(DVec3::Z);
        if ng.dot(d) > 0.0 {
            ng = -ng;
        }
        let mut n = (t.n[0] * w0 + t.n[1] * u + t.n[2] * v).normalize_or(ng);
        if n.dot(ng) < 0.0 {
            n = -n;
        }
        let info = &ctx.items[t.item as usize];
        let stretch = (info.material.anisotropy > 0.0).then(|| brush_stretch(n, p, info.axis, info.origin));
        let s =
            Surf { p, n, view: -d, base: info.albedo, m: info.material, stretch, ao: 1.0, phi: None, ground: false };
        ctx.shade(&s, None).0
    }
}

/// Shades the opaque samples of `t` from the G-buffer (see the module docs). `tris`: the
/// opaque (clipped) triangles, `items`: per scene item.
#[allow(clippy::too_many_arguments)]
pub(crate) fn shade(
    t: &mut Target,
    g: &mut GBuf,
    cam: &Camera,
    opts: &RenderOptions,
    st: &Studio,
    tris: &[RtTri],
    items: &[ItemInfo],
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
    let sm =
        if st.shadows && !lights.is_empty() && light.z > -0.99 { ShadowMap::build(tris, light, sm_size) } else { None };
    let tan_theta = (st.shadow_softness.clamp(0.0, 1.0) * MAX_LIGHT_RADIUS_DEG).to_radians().tan();

    // Environment.
    let amb = st.ambient.clamp(0.0, 1.0);
    let light_w = (1.0 - amb) * st.light_intensity.max(0.0);
    let gc = st.ground_color;
    let ground_lin = [gc[0], gc[1], gc[2]].map(|c| srgb_to_linear(c) as f64);
    let spec_env = environment_spec(st.environment, ground_lin);
    let maps = (st.environment != Environment::None).then(|| EnvMaps::cached(&spec_env));
    let env_spec_w = amb * ENV_SPECULAR_GAIN;
    let env_diff_w = amb / up_irradiance(&spec_env).max(1e-6);
    let cam_az = camera_azimuth(cam.dir, cam.up);
    let fade = (diag * 0.55, diag * 1.5);
    let center_xy = DVec3::new(center.x, center.y, 0.0);

    // Ray tracing: reflections of the scene in glossy surfaces and in the ground.
    let (rays_surf, rays_ground) = st.quality.rays();
    let rays = (
        if st.reflections && maps.is_some() { rays_surf } else { 0 },
        if st.ground_reflection > 0.0 && ground_z.is_some() { rays_ground.max(1) } else { 0 },
    );
    let bvh = (rays != (0, 0) && !tris.is_empty()).then(|| Bvh::build(tris));
    let tracer = bvh.as_ref().map(|bvh| Tracer {
        bvh,
        tris,
        eps: diag * 2e-4,
        rays,
        sparse: st.quality != Quality::High,
        ground: ground_z.map(|z| GroundInfo { z, center: center_xy, fade }),
        shadow_share: if lights.is_empty() { 0.0 } else { (1.0 - amb).clamp(0.0, 1.0) * 0.85 },
    });
    let tracer_sparse = tracer.as_ref().is_some_and(|t| t.sparse);
    let ctx = Ctx {
        lights: &lights,
        light_w,
        sm: sm.as_ref(),
        tan_theta,
        maps: maps.as_deref(),
        rot: ((-cam_az).cos(), (-cam_az).sin()),
        env_spec_w,
        env_diff_w,
        amb,
        bounce: std::array::from_fn(|k| 0.35 + 0.4 * ground_lin[k]),
        dfg: Dfg::get(),
        items,
        tracer: tracer.as_ref(),
    };

    // Lighting (linear light); glossy reflections of the scene separately in `refl`.
    let mut shaded = vec![([0.0f32; 3], [0.0f32; 4]); w * h];
    {
        let (g, ao) = (&*g, &ao);
        let (depth, gdepth, ctx) = (&t.depth, &gdepth, &ctx);
        par_rows(&mut shaded, w, |y0, rows| {
            // the albedo changes rarely along a row: convert it once per change
            let mut last_albedo = ([f32::NAN; 3], [0.0f64; 3]);
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
                if g.albedo[i] != last_albedo.0 {
                    last_albedo = (g.albedo[i], g.albedo[i].map(|c| srgb_to_linear(c) as f64));
                }
                let (x, y) = (i % w, i / w);
                let phi = ign(x, y) * std::f64::consts::TAU;
                let a = ao.as_ref().map_or(1.0, |v| v[i] as f64);
                let (base, m, stretch) = if kind == SURFACE {
                    let info = &ctx.items[g.item[i] as usize];
                    let m = info.material;
                    let base = match m.color {
                        Some(c) => c.map(|v| v as f64),
                        None => last_albedo.1,
                    };
                    let stretch = (m.anisotropy > 0.0).then(|| brush_stretch(n, p, info.axis, info.origin));
                    (base, m, stretch)
                } else {
                    (last_albedo.1, Material { roughness: 1.0, ..Material::PLASTIC }, None)
                };
                let s = Surf { p, n, view, base, m, stretch, ao: a, phi: Some(phi), ground: kind == GROUND };
                // ray pattern: 4 × 4 offsets and a rotation per sample
                let take = !tracer_sparse || (x % 2 == 0 && y % 2 == 0);
                let trace = (kind == SURFACE).then(|| (ign(x / 2, y / 2), ign(x / 2 + 37, y / 2 + 11), take));
                let (c, r) = ctx.shade(&s, trace);
                *out = (
                    c.map(|v| v as f32),
                    match r {
                        None => [0.0; 4],
                        Some(r) if r[0].is_nan() => [0.0, 0.0, 0.0, 0.5],
                        Some(r) => [r[0] as f32, r[1] as f32, r[2] as f32, 1.0],
                    },
                );
            }
        });
    }

    // Glossy reflections: smoothed with a Gaussian over the traced samples nearby (wider for
    // rougher surfaces) of similar normal and on the sample's tangent plane.
    let mut lin: Vec<[f32; 3]> = shaded.iter().map(|s| s.0).collect();
    if shaded.iter().any(|s| s.1[3] > 0.0) {
        let tol = diag * 0.004;
        let pos: Vec<DVec3> = (0..w * h)
            .map(|i| {
                let (sx, sy) = sample(i);
                if g.kind[i] == SURFACE { cam.unproject(sx, sy, t.depth[i]) } else { DVec3::ZERO }
            })
            .collect();
        let (g, shaded, pos, ctx) = (&*g, &shaded, &pos, &ctx);
        par_rows(&mut lin, w, |y0, rows| {
            for (j, out) in rows.iter_mut().enumerate() {
                let i = y0 * w + j;
                if shaded[i].1[3] <= 0.0 {
                    continue;
                }
                let (x, y) = ((i % w) as i64, (i / w) as i64);
                let ni = g.normal[i];
                let rough = ctx.items[g.item[i] as usize].material.roughness as f64;
                // taps on the lattice of the traced samples around the sample
                let lat = if tracer_sparse { 2 } else { 1 };
                let k = 3 + (rough * 4.0).round() as i64;
                let sigma = (k * lat) as f64 * 0.5;
                let (bx, by) = (x - x.rem_euclid(lat), y - y.rem_euclid(lat));
                let (mut s, mut n) = ([0.0f32; 3], 0.0f32);
                for dy in -k..=k {
                    for dx in -k..=k {
                        let (xx, yy) = (bx + dx * lat, by + dy * lat);
                        if xx < 0 || yy < 0 || xx >= w as i64 || yy >= h as i64 {
                            continue;
                        }
                        let q = yy as usize * w + xx as usize;
                        if shaded[q].1[3] < 1.0 {
                            continue;
                        }
                        let nq = g.normal[q];
                        if ni[0] * nq[0] + ni[1] * nq[1] + ni[2] * nq[2] < 0.9 {
                            continue;
                        }
                        // near the tangent plane of the sample (same face or smooth surface)
                        if (pos[q] - pos[i]).dot(v3f(ni)).abs() > tol {
                            continue;
                        }
                        let d2 = ((xx - x).pow(2) + (yy - y).pow(2)) as f64;
                        let wt = (-d2 / (2.0 * sigma * sigma)).exp() as f32;
                        for (v, r) in s.iter_mut().zip(shaded[q].1) {
                            *v += r * wt;
                        }
                        n += wt;
                    }
                }
                for c in 0..3 {
                    out[c] = (out[c] + if n > 0.0 { s[c] / n } else { 0.0 }).max(0.0);
                }
            }
        });
    }
    drop(shaded);

    // Mirror image of the model in the ground.
    if let (Some(tr), Some(gz)) = (ctx.tracer, ground_z)
        && rays.1 > 0
    {
        let strength = st.ground_reflection.clamp(0.0, 1.0);
        let alpha = (st.ground_roughness.clamp(0.0, 1.0) * 0.5).powi(2).max(1e-6);
        let k = if alpha < 1e-4 { 1 } else { rays.1 };
        let fade_h = diag * 0.6;
        let mut layer = vec![[0.0f32; 4]; w * h];
        {
            let (g, gdepth, ctx) = (&*g, &gdepth, &ctx);
            par_rows(&mut layer, w, |y0, rows| {
                for (j, out) in rows.iter_mut().enumerate() {
                    let i = y0 * w + j;
                    if g.kind[i] != GROUND {
                        continue;
                    }
                    let (sx, sy) = sample(i);
                    let p = cam.unproject(sx, sy, gdepth[i]);
                    if smoothstep(fade.0, fade.1, (DVec3::new(p.x, p.y, 0.0) - center_xy).length()) >= 0.999 {
                        continue;
                    }
                    let view = -cam.ray(p);
                    let o = DVec3::new(p.x, p.y, gz + tr.eps);
                    let (x, y) = (i % w, i / w);
                    if tr.sparse && (x % 2 != 0 || y % 2 != 0) {
                        // filled by the blur
                        *out = [0.0, 0.0, 0.0, -1.0];
                        continue;
                    }
                    let (u, phi) = (ign(x / 2, y / 2), ign(x / 2 + 37, y / 2 + 11));
                    let mut acc = [0.0f64; 4];
                    for s in 0..k {
                        let d = if k == 1 {
                            DVec3::new(-view.x, -view.y, view.z)
                        } else {
                            let u1 = ((s as f64 + u) / k as f64).fract();
                            let u2 = (s as f64 * 0.618_034 + phi).fract();
                            let hh = ggx_half(u1, u2, alpha, DVec3::Z, DVec3::X, DVec3::Y);
                            let l = hh * (2.0 * view.dot(hh)) - view;
                            if l.z <= 1e-3 {
                                continue;
                            }
                            l.normalize()
                        };
                        let Some(hit) = tr.bvh.intersect(o, d, 0.0, fade_h / d.z) else { continue };
                        let f = 1.0 - smoothstep(0.0, fade_h, hit.t * d.z);
                        if f <= 0.0 {
                            continue;
                        }
                        let c = tr.shade_hit(ctx, hit.tri as usize, hit.u, hit.v, d);
                        for q in 0..3 {
                            acc[q] += c[q] * f;
                        }
                        acc[3] += f;
                    }
                    *out = acc.map(|v| (v / k as f64) as f32);
                }
            });
        }
        // blur over 5 × 5 ground samples (smooths the ray noise), then over the ground colour
        let mut blurred = vec![[0.0f32; 4]; w * h];
        {
            let (g, layer) = (&*g, &layer);
            par_rows(&mut blurred, w, |y0, rows| {
                for (j, out) in rows.iter_mut().enumerate() {
                    let i = y0 * w + j;
                    if g.kind[i] != GROUND {
                        continue;
                    }
                    let (x, y) = ((i % w) as i64, (i / w) as i64);
                    let (mut s, mut n) = ([0.0f32; 4], 0.0f32);
                    for dy in -2i64..=2 {
                        for dx in -2i64..=2 {
                            let (xx, yy) = (x + dx, y + dy);
                            if xx < 0 || yy < 0 || xx >= w as i64 || yy >= h as i64 {
                                continue;
                            }
                            let q = yy as usize * w + xx as usize;
                            if g.kind[q] != GROUND || layer[q][3] < 0.0 {
                                continue;
                            }
                            for c in 0..4 {
                                s[c] += layer[q][c];
                            }
                            n += 1.0;
                        }
                    }
                    *out = s.map(|v| v / n.max(1.0));
                }
            });
        }
        for i in 0..w * h {
            if g.kind[i] != GROUND || blurred[i][3] <= 0.0 {
                continue;
            }
            let (sx, sy) = sample(i);
            let p = cam.unproject(sx, sy, gdepth[i]);
            let n_dot_v = (-cam.ray(p)).z.clamp(0.0, 1.0);
            let f = (strength + (1.0 - strength) * (1.0 - n_dot_v).powi(5) * strength) as f32;
            let b = blurred[i];
            for c in 0..3 {
                lin[i][c] = lin[i][c] * (1.0 - f * b[3]) + f * b[c];
            }
        }
    }

    // Tone mapping, sRGB, the ground fading into the background.
    let srgb = SrgbTable::new();
    {
        let (g, lin, srgb, gdepth) = (&*g, &lin, &srgb, &gdepth);
        par_rows(&mut t.color, w, |y0, rows| {
            for (j, out) in rows.iter_mut().enumerate() {
                let i = y0 * w + j;
                let kind = g.kind[i];
                if kind == NONE {
                    continue;
                }
                let c = neutral_tone(lin[i].map(|v| v as f64)).map(|v| srgb.get(v));
                *out = if kind == GROUND {
                    let (sx, sy) = sample(i);
                    let p = cam.unproject(sx, sy, gdepth[i]);
                    let f = smoothstep(fade.0, fade.1, (DVec3::new(p.x, p.y, 0.0) - center_xy).length()) as f32;
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
        rays: tracer.as_ref().map(|t| t.rays),
    }
}
