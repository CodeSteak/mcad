//! The procedural environment of the studio look: what the surroundings of the model look like
//! in reflections and ambient light.
//!
//! * Definition ([`EnvSpec`]): a vertical gradient (floor near the model → dark horizon → walls
//!   → ceiling) plus rectangular softboxes with soft edges ([`Softbox`]) in high dynamic range
//!   (many times brighter than the walls). Directions are relative to the camera like the
//!   lights. The UI builds its environment map from the same numbers and the same function
//!   (`web/src/logic/studio.ts`, tests compare them).
//! * Maps ([`EnvMaps`]): the radiance in an octahedral map, prefiltered for a set of GGX
//!   roughness levels by filtered importance sampling (the split-sum approximation), and the
//!   diffuse irradiance (cosine convolution). Built once per environment and ground colour
//!   and cached for the process; deterministic (fixed sample sets).
//! * The split-sum DFG table ([`Dfg`]): scale and bias of F0 for the environment BRDF and the
//!   single-scattering energy (multiple-scattering compensation), computed numerically.
//!
//! No image files and no textures: everything comes from these functions.

use std::sync::{Arc, Mutex, OnceLock};

use glam::{DVec3, Vec3};

use crate::studio::{Environment, light_direction};

/// A rectangular softbox of the environment, facing the model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Softbox {
    /// Degrees relative to the camera (as for lights): 0 = behind the camera, + = right.
    pub azimuth: f64,
    /// Degrees above the horizon.
    pub elevation: f64,
    /// Angular size in degrees: horizontal, vertical.
    pub width: f64,
    pub height: f64,
    /// Width of the soft edge in degrees (the light falls off over ± this around the edge).
    pub softness: f64,
    /// Linear RGB radiance at the centre (the edges are 20 % darker).
    pub radiance: [f64; 3],
}

/// The environment: a vertical gradient and softboxes (linear RGB radiance).
#[derive(Debug, Clone, PartialEq)]
pub struct EnvSpec {
    /// Straight up.
    pub zenith: [f64; 3],
    /// Walls at the horizon.
    pub horizon: [f64; 3],
    /// Floor far away (just below the horizon).
    pub floor_horizon: [f64; 3],
    /// Floor straight below the model (rising with the squared sine of the depression: dark
    /// far away, lit near the model).
    pub nadir: [f64; 3],
    /// Half width of the transition floor → wall in degrees.
    pub horizon_blur: f64,
    pub boxes: Vec<Softbox>,
}

/// Factor of the environment reflections per unit of `ambient` (radiance as in
/// [`environment_spec`]). The diffuse environment light is normalised instead: a white surface
/// facing up receives `ambient` (as the hemisphere light of the classic studio look).
pub const ENV_SPECULAR_GAIN: f64 = 2.5;

/// The environment `env` around a ground of linear colour `ground`.
pub fn environment_spec(env: Environment, ground: [f64; 3]) -> EnvSpec {
    let g = |k: f64| ground.map(|c| c * k);
    let box_ = |azimuth: f64, elevation: f64, width: f64, height: f64, softness: f64, radiance: [f64; 3]| Softbox {
        azimuth,
        elevation,
        width,
        height,
        softness,
        radiance,
    };
    match env {
        // a dark studio: near-black walls, a lit floor, large HDR softboxes — metals show
        // contrast (bright highlights with crisp edges against dark surroundings)
        Environment::Studio => EnvSpec {
            zenith: [0.05, 0.05, 0.055],
            horizon: [0.012, 0.012, 0.014],
            floor_horizon: g(0.05),
            nadir: g(0.75),
            horizon_blur: 3.0,
            boxes: vec![
                // large softbox above, behind the model (top faces, rims)
                box_(180.0, 55.0, 64.0, 30.0, 5.0, [6.0, 6.0, 6.0]),
                // tall strips standing on the floor (walls seen from above reflect them too):
                // key strip on the left, slightly warm; fill strip on the right, slightly cool
                box_(-68.0, 5.0, 16.0, 100.0, 3.0, [9.0, 8.6, 8.0]),
                box_(105.0, 5.0, 12.0, 96.0, 3.0, [3.0, 3.15, 3.4]),
                // a low, wide card behind the camera: front faces get a soft gradient
                box_(0.0, 8.0, 110.0, 22.0, 14.0, [0.45, 0.45, 0.47]),
                // a white bounce card low on the right: the side away from the key light is
                // not lost in the dark (form of flat faces)
                box_(95.0, -22.0, 60.0, 36.0, 18.0, [0.3, 0.3, 0.3]),
            ],
        },
        // a light studio: grey walls, large soft boxes, low contrast
        Environment::Soft => EnvSpec {
            zenith: [0.55, 0.56, 0.58],
            horizon: [0.3, 0.3, 0.31],
            floor_horizon: g(0.35),
            nadir: g(0.75),
            horizon_blur: 10.0,
            boxes: vec![
                box_(180.0, 55.0, 110.0, 60.0, 18.0, [2.2, 2.2, 2.2]),
                box_(-70.0, 22.0, 34.0, 60.0, 12.0, [2.6, 2.55, 2.45]),
                box_(105.0, 22.0, 30.0, 56.0, 12.0, [1.6, 1.65, 1.7]),
            ],
        },
        // sky gradient only: soft, even light
        Environment::Overcast => EnvSpec {
            zenith: [1.0, 1.01, 1.05],
            horizon: [0.45, 0.45, 0.46],
            floor_horizon: g(0.35),
            nadir: g(0.6),
            horizon_blur: 12.0,
            boxes: Vec::new(),
        },
        Environment::None => EnvSpec {
            zenith: [0.0; 3],
            horizon: [0.0; 3],
            floor_horizon: [0.0; 3],
            nadir: [0.0; 3],
            horizon_blur: 1.0,
            boxes: Vec::new(),
        },
    }
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A softbox placed for the camera at azimuth 0: centre direction, horizontal and vertical
/// axes.
struct Placed {
    c: DVec3,
    u: DVec3,
    v: DVec3,
    b: Softbox,
}

/// An [`EnvSpec`] ready for evaluation (camera at azimuth 0: looking along +y).
pub struct EnvFn {
    spec: EnvSpec,
    boxes: Vec<Placed>,
}

/// Direction of `azimuth`, `elevation` for the camera at azimuth 0 (looking along +y).
fn canonical_dir(azimuth: f64, elevation: f64) -> DVec3 {
    light_direction(DVec3::Y, DVec3::Z, azimuth, elevation)
}

impl EnvFn {
    pub fn new(spec: &EnvSpec) -> EnvFn {
        let boxes = spec
            .boxes
            .iter()
            .map(|b| {
                let c = canonical_dir(b.azimuth, b.elevation);
                let u = if c.z.abs() < 0.999 {
                    DVec3::Z.cross(c).normalize()
                } else {
                    let a = b.azimuth.to_radians();
                    DVec3::new(a.cos(), a.sin(), 0.0)
                };
                let v = c.cross(u);
                Placed { c, u, v, b: *b }
            })
            .collect();
        EnvFn { spec: spec.clone(), boxes }
    }

    /// Radiance seen in direction `d` (unit; camera at azimuth 0).
    pub fn radiance(&self, d: DVec3) -> [f64; 3] {
        let s = &self.spec;
        let el = d.z.clamp(-1.0, 1.0).asin().to_degrees();
        let up = d.z.max(0.0);
        let down = d.z.min(0.0).powi(2);
        let k = smoothstep(-s.horizon_blur, s.horizon_blur, el);
        let mut c: [f64; 3] = std::array::from_fn(|i| {
            let wall = s.horizon[i] + (s.zenith[i] - s.horizon[i]) * up;
            let floor = s.floor_horizon[i] + (s.nadir[i] - s.floor_horizon[i]) * down;
            floor + (wall - floor) * k
        });
        for p in &self.boxes {
            let z = d.dot(p.c);
            if z <= 1e-3 {
                continue;
            }
            let x = d.dot(p.u).atan2(z).to_degrees();
            let y = d.dot(p.v).atan2(z).to_degrees();
            let (hw, hh, sf) = (p.b.width / 2.0, p.b.height / 2.0, p.b.softness.max(0.01));
            let m = (1.0 - smoothstep(hw - sf, hw + sf, x.abs())) * (1.0 - smoothstep(hh - sf, hh + sf, y.abs()));
            if m <= 0.0 {
                continue;
            }
            let hot = 1.0 - 0.2 * ((x / hw).powi(2) + (y / hh).powi(2)).min(1.0);
            for (v, r) in c.iter_mut().zip(p.b.radiance) {
                *v += r * m * hot;
            }
        }
        c
    }
}

/// Irradiance of `spec` on a surface facing up, divided by π (luminance; 1 for a uniform
/// radiance 1). Midpoint rule over 256 × 64 directions of the upper hemisphere — the UI computes
/// the same sum (`upIrradiance`).
pub fn up_irradiance(spec: &EnvSpec) -> f64 {
    let f = EnvFn::new(spec);
    let (na, ne) = (256usize, 64usize);
    let (da, de) = (std::f64::consts::TAU / na as f64, std::f64::consts::FRAC_PI_2 / ne as f64);
    let mut e = [0.0f64; 3];
    for j in 0..ne {
        let el = (j as f64 + 0.5) * de;
        let w = el.sin() * el.cos() * de * da;
        for i in 0..na {
            let a = (i as f64 + 0.5) * da;
            let d = DVec3::new(a.cos() * el.cos(), a.sin() * el.cos(), el.sin());
            let l = f.radiance(d);
            for k in 0..3 {
                e[k] += l[k] * w;
            }
        }
    }
    (0.2126 * e[0] + 0.7152 * e[1] + 0.0722 * e[2]) / std::f64::consts::PI
}

/// Azimuth (radians) of the camera around z, as [`light_direction`] uses it: 0 = from −y.
pub fn camera_azimuth(cam_dir: DVec3, cam_up: DVec3) -> f64 {
    let eye = -cam_dir;
    if eye.x.abs() < 1e-9 && eye.y.abs() < 1e-9 {
        let e = -cam_up * eye.z.signum();
        e.x.atan2(-e.y)
    } else {
        eye.x.atan2(-eye.y)
    }
}

// ---------------------------------------------------------------------------------------------
// Octahedral maps

fn sign(x: f32) -> f32 {
    if x >= 0.0 { 1.0 } else { -1.0 }
}

/// Unit direction → octahedral coordinates in [-1, 1]².
fn oct_encode(d: Vec3) -> (f32, f32) {
    let s = d.x.abs() + d.y.abs() + d.z.abs();
    let (x, y) = (d.x / s, d.y / s);
    if d.z >= 0.0 { (x, y) } else { ((1.0 - y.abs()) * sign(x), (1.0 - x.abs()) * sign(y)) }
}

fn oct_decode(x: f32, y: f32) -> Vec3 {
    let z = 1.0 - x.abs() - y.abs();
    let (px, py) = if z >= 0.0 { (x, y) } else { ((1.0 - y.abs()) * sign(x), (1.0 - x.abs()) * sign(y)) };
    Vec3::new(px, py, z).normalize()
}

/// A square octahedral map of `n` × `n` linear RGB texels.
#[derive(Clone)]
struct OctMap {
    n: usize,
    texels: Vec<[f32; 3]>,
}

impl OctMap {
    /// Direction of the texel centre (i, j).
    fn dir(n: usize, i: usize, j: usize) -> Vec3 {
        let f = |k: usize| (k as f32 + 0.5) / n as f32 * 2.0 - 1.0;
        oct_decode(f(i), f(j))
    }

    fn texel(&self, i: i64, j: i64) -> [f32; 3] {
        // the border of an octahedral map continues mirrored on the same edge
        let n = self.n as i64;
        let (mut i, mut j) = (i, j);
        if i < 0 {
            i = -i - 1;
            j = n - 1 - j;
        } else if i >= n {
            i = 2 * n - 1 - i;
            j = n - 1 - j;
        }
        if j < 0 {
            j = -j - 1;
            i = n - 1 - i;
        } else if j >= n {
            j = 2 * n - 1 - j;
            i = n - 1 - i;
        }
        self.texels[(j.clamp(0, n - 1) * n + i.clamp(0, n - 1)) as usize]
    }

    /// Bilinear lookup in direction `d` (unit).
    fn sample(&self, d: Vec3) -> [f32; 3] {
        let (x, y) = oct_encode(d);
        let n = self.n as f32;
        let fx = (x + 1.0) * 0.5 * n - 0.5;
        let fy = (y + 1.0) * 0.5 * n - 0.5;
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let a = self.texel(x0, y0);
        let b = self.texel(x0 + 1, y0);
        let c = self.texel(x0, y0 + 1);
        let e = self.texel(x0 + 1, y0 + 1);
        std::array::from_fn(|k| {
            let top = a[k] + (b[k] - a[k]) * tx;
            let bot = c[k] + (e[k] - c[k]) * tx;
            top + (bot - top) * ty
        })
    }

    /// Half the size (2 × 2 box filter).
    fn half(&self) -> OctMap {
        let n = (self.n / 2).max(1);
        let mut texels = vec![[0.0f32; 3]; n * n];
        for j in 0..n {
            for i in 0..n {
                let mut s = [0.0f32; 3];
                for (di, dj) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let t = self.texels[(2 * j + dj).min(self.n - 1) * self.n + (2 * i + di).min(self.n - 1)];
                    for k in 0..3 {
                        s[k] += t[k] * 0.25;
                    }
                }
                texels[j * n + i] = s;
            }
        }
        OctMap { n, texels }
    }
}

/// Runs `f(texel index)` for all `n × n` texels in parallel.
fn par_texels(n: usize, f: impl Fn(usize) -> [f32; 3] + Sync) -> Vec<[f32; 3]> {
    let mut out = vec![[0.0f32; 3]; n * n];
    let threads = std::thread::available_parallelism().map_or(1, |t| t.get()).clamp(1, 16);
    let chunk = (n * n).div_ceil(threads * 4).max(64);
    let f = &f;
    std::thread::scope(|s| {
        for (k, part) in out.chunks_mut(chunk).enumerate() {
            s.spawn(move || {
                for (j, o) in part.iter_mut().enumerate() {
                    *o = f(k * chunk + j);
                }
            });
        }
    });
    out
}

/// Point `i` of `n` of the Hammersley set.
fn hammersley(i: u32, n: u32) -> (f32, f32) {
    (i as f32 / n as f32, (i.reverse_bits() as f64 / 4294967296.0) as f32)
}

/// Half vector of the GGX distribution (`alpha` = roughness²) for the sample `(u1, u2)`, in
/// the frame (t, b, n).
fn ggx_half(u1: f32, u2: f32, alpha: f32) -> Vec3 {
    let a2 = alpha * alpha;
    let cos_t = ((1.0 - u1) / (1.0 + (a2 - 1.0) * u1)).max(0.0).sqrt();
    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
    let phi = std::f32::consts::TAU * u2;
    Vec3::new(sin_t * phi.cos(), sin_t * phi.sin(), cos_t)
}

fn frame(n: Vec3) -> (Vec3, Vec3) {
    n.any_orthonormal_pair()
}

/// Roughness of the prefiltered levels and their sizes.
const LEVELS: [(f32, usize, u32); 8] = [
    (0.0, 256, 0),
    (0.1, 256, 24),
    (0.2, 128, 64),
    (0.3, 64, 96),
    (0.45, 64, 128),
    (0.6, 32, 128),
    (0.8, 32, 128),
    (1.0, 16, 128),
];

/// Base map size.
const BASE: usize = 256;

/// The environment as prefiltered maps (camera at azimuth 0; see the module docs).
pub struct EnvMaps {
    levels: Vec<OctMap>,
    irradiance: OctMap,
}

impl EnvMaps {
    pub fn build(spec: &EnvSpec) -> EnvMaps {
        let f = EnvFn::new(spec);
        // base: 2 × 2 samples per texel (the softbox edges stay smooth)
        let base = OctMap {
            n: BASE,
            texels: par_texels(BASE, |t| {
                let (i, j) = (t % BASE, t / BASE);
                let mut s = [0.0f64; 3];
                for (ox, oy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                    let x = (i as f32 + ox) / BASE as f32 * 2.0 - 1.0;
                    let y = (j as f32 + oy) / BASE as f32 * 2.0 - 1.0;
                    let d = oct_decode(x, y).as_dvec3();
                    let l = f.radiance(d);
                    for k in 0..3 {
                        s[k] += l[k] * 0.25;
                    }
                }
                s.map(|v| v as f32)
            }),
        };
        let mut mips = vec![base];
        while mips.last().is_some_and(|m| m.n > 1) {
            let h = mips.last().map(OctMap::half).expect("mip");
            mips.push(h);
        }
        let sample_mip = |d: Vec3, lod: f32| -> [f32; 3] {
            let lod = lod.clamp(0.0, (mips.len() - 1) as f32);
            let l0 = lod.floor() as usize;
            let t = lod - l0 as f32;
            let a = mips[l0].sample(d);
            if t <= 0.0 || l0 + 1 >= mips.len() {
                return a;
            }
            let b = mips[l0 + 1].sample(d);
            std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t)
        };
        let texel_solid = 4.0 * std::f32::consts::PI / (BASE * BASE) as f32;
        let mut levels = Vec::with_capacity(LEVELS.len());
        for &(rough, n, count) in &LEVELS {
            if count == 0 {
                levels.push(mips[0].clone());
                continue;
            }
            let alpha = rough * rough;
            let a2 = alpha * alpha;
            let samples: Vec<(Vec3, f32)> = (0..count)
                .filter_map(|i| {
                    let (u1, u2) = hammersley(i, count);
                    let h = ggx_half(u1, u2, alpha);
                    // N = V: L = 2 (N·H) H − N
                    let l = Vec3::new(2.0 * h.z * h.x, 2.0 * h.z * h.y, 2.0 * h.z * h.z - 1.0);
                    if l.z <= 0.0 {
                        return None;
                    }
                    let dd = h.z * h.z * (a2 - 1.0) + 1.0;
                    let d = a2 / (std::f32::consts::PI * dd * dd);
                    let pdf = d / 4.0;
                    let omega_s = 1.0 / (count as f32 * pdf.max(1e-6));
                    let lod = (0.5 * (omega_s / texel_solid).log2() + 1.0).max(0.0);
                    Some((l, lod))
                })
                .collect();
            let texels = par_texels(n, |t| {
                let nrm = OctMap::dir(n, t % n, t / n);
                let (tx, ty) = frame(nrm);
                let (mut s, mut w) = ([0.0f32; 3], 0.0f32);
                for (l, lod) in &samples {
                    let d = tx * l.x + ty * l.y + nrm * l.z;
                    let c = sample_mip(d, *lod);
                    for k in 0..3 {
                        s[k] += c[k] * l.z;
                    }
                    w += l.z;
                }
                s.map(|v| v / w.max(1e-6))
            });
            levels.push(OctMap { n, texels });
        }
        // irradiance: cosine convolution over the 32 × 32 mip (solid angle per texel)
        let src = &mips[mips.iter().position(|m| m.n == 32).unwrap_or(mips.len() - 1)];
        let sn = src.n;
        let (dirs, solid): (Vec<Vec3>, Vec<f32>) = {
            let mut dirs = Vec::with_capacity(sn * sn);
            let mut solid = Vec::with_capacity(sn * sn);
            let h = 1.0 / sn as f32;
            for j in 0..sn {
                for i in 0..sn {
                    let x = (i as f32 + 0.5) * 2.0 * h - 1.0;
                    let y = (j as f32 + 0.5) * 2.0 * h - 1.0;
                    let d = oct_decode(x, y);
                    let dx = oct_decode((x + h).min(1.0), y) - oct_decode((x - h).max(-1.0), y);
                    let dy = oct_decode(x, (y + h).min(1.0)) - oct_decode(x, (y - h).max(-1.0));
                    dirs.push(d);
                    solid.push(dx.cross(dy).length());
                }
            }
            let sum: f32 = solid.iter().sum();
            let k = 4.0 * std::f32::consts::PI / sum;
            (dirs, solid.into_iter().map(|s| s * k).collect())
        };
        let irr_n = 32;
        let irradiance = OctMap {
            n: irr_n,
            texels: par_texels(irr_n, |t| {
                let nrm = OctMap::dir(irr_n, t % irr_n, t / irr_n);
                let mut s = [0.0f32; 3];
                for (k, d) in dirs.iter().enumerate() {
                    let c = nrm.dot(*d);
                    if c > 0.0 {
                        let w = c * solid[k];
                        for (q, v) in s.iter_mut().enumerate() {
                            *v += src.texels[k][q] * w;
                        }
                    }
                }
                s
            }),
        };
        EnvMaps { levels, irradiance }
    }

    /// The maps of `spec`, built once per process (a few environments are kept).
    pub fn cached(spec: &EnvSpec) -> Arc<EnvMaps> {
        type Cache = Mutex<Vec<(EnvSpec, Arc<EnvMaps>)>>;
        static CACHE: OnceLock<Cache> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(Vec::new()));
        if let Some((_, m)) = cache.lock().ok().and_then(|c| c.iter().find(|(s, _)| s == spec).cloned()) {
            return m;
        }
        let maps = Arc::new(EnvMaps::build(spec));
        if let Ok(mut c) = cache.lock() {
            if c.len() >= 4 {
                c.remove(0);
            }
            c.push((spec.clone(), maps.clone()));
        }
        maps
    }

    /// Radiance around direction `d` averaged over the GGX lobe of perceptual `roughness`.
    pub fn specular(&self, d: DVec3, roughness: f64) -> [f64; 3] {
        let r = roughness.clamp(0.0, 1.0) as f32;
        let k = LEVELS.iter().position(|l| l.0 >= r).unwrap_or(LEVELS.len() - 1).max(1);
        let (r0, r1) = (LEVELS[k - 1].0, LEVELS[k].0);
        let t = ((r - r0) / (r1 - r0)).clamp(0.0, 1.0);
        let d = d.as_vec3();
        let a = self.levels[k - 1].sample(d);
        if t <= 0.0 {
            return a.map(|v| v as f64);
        }
        let b = self.levels[k].sample(d);
        std::array::from_fn(|i| (a[i] + (b[i] - a[i]) * t) as f64)
    }

    /// Irradiance on a surface with normal `n` (`∫ L(ω) max(0, n·ω) dω`).
    pub fn irradiance(&self, n: DVec3) -> [f64; 3] {
        self.irradiance.sample(n.as_vec3()).map(|v| v as f64)
    }
}

// ---------------------------------------------------------------------------------------------
// DFG table

/// Height-correlated Smith visibility `V = G / (4 n·l n·v)` (`alpha` = roughness²).
pub(crate) fn smith_vis(n_dot_l: f64, n_dot_v: f64, alpha: f64) -> f64 {
    let a2 = alpha * alpha;
    let gv = n_dot_l * (n_dot_v * n_dot_v * (1.0 - a2) + a2).sqrt();
    let gl = n_dot_v * (n_dot_l * n_dot_l * (1.0 - a2) + a2).sqrt();
    0.5 / (gv + gl).max(1e-12)
}

/// The split-sum environment BRDF: `(scale, bias)` of F0 over (roughness, n·v), 32 × 32
/// entries, 512 importance samples each (like three.js `dfgLUT`).
pub struct Dfg {
    t: Vec<(f32, f32)>,
}

const DFG_N: usize = 32;

impl Dfg {
    pub fn get() -> &'static Dfg {
        static DFG: OnceLock<Dfg> = OnceLock::new();
        DFG.get_or_init(Dfg::build)
    }

    fn build() -> Dfg {
        const S: u32 = 512;
        let mut t = Vec::with_capacity(DFG_N * DFG_N);
        for j in 0..DFG_N {
            let nv = ((j as f64 + 0.5) / DFG_N as f64).max(1e-3);
            let v = DVec3::new((1.0 - nv * nv).sqrt(), 0.0, nv);
            for i in 0..DFG_N {
                let rough = (i as f64 + 0.5) / DFG_N as f64;
                let alpha = rough * rough;
                let (mut a, mut b) = (0.0, 0.0);
                for k in 0..S {
                    let (u1, u2) = hammersley(k, S);
                    let h = ggx_half(u1, u2, alpha as f32).as_dvec3();
                    let vh = v.dot(h);
                    let l = h * (2.0 * vh) - v;
                    if l.z <= 0.0 || vh <= 0.0 {
                        continue;
                    }
                    let g = smith_vis(l.z, nv, alpha) * 4.0 * l.z * vh / h.z.max(1e-9);
                    let fc = (1.0 - vh).powi(5);
                    a += (1.0 - fc) * g;
                    b += fc * g;
                }
                t.push(((a / S as f64) as f32, (b / S as f64) as f32));
            }
        }
        Dfg { t }
    }

    /// `(scale, bias)`: the directional albedo is `F0 · scale + F90 · bias`.
    pub fn lookup(&self, roughness: f64, n_dot_v: f64) -> (f64, f64) {
        let n = DFG_N as f64;
        let fx = (roughness.clamp(0.0, 1.0) * n - 0.5).clamp(0.0, n - 1.0);
        let fy = (n_dot_v.clamp(0.0, 1.0) * n - 0.5).clamp(0.0, n - 1.0);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(DFG_N - 1), (y0 + 1).min(DFG_N - 1));
        let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
        let at = |x: usize, y: usize| self.t[y * DFG_N + x];
        let lerp = |a: (f32, f32), b: (f32, f32), t: f64| {
            (a.0 as f64 + (b.0 as f64 - a.0 as f64) * t, a.1 as f64 + (b.1 as f64 - a.1 as f64) * t)
        };
        let top = lerp(at(x0, y0), at(x1, y0), tx);
        let bot = lerp(at(x0, y1), at(x1, y1), tx);
        (top.0 + (bot.0 - top.0) * ty, top.1 + (bot.1 - top.1) * ty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn octahedral_round_trip_and_border() {
        for d in [Vec3::X, Vec3::NEG_Z, Vec3::new(0.3, -0.5, -0.8).normalize(), Vec3::new(-0.2, 0.9, 0.1).normalize()] {
            let (x, y) = oct_encode(d);
            assert!((oct_decode(x, y) - d).length() < 1e-5, "{d}");
        }
        // texels across the border are neighbours on the sphere
        let n = 16;
        let m = OctMap { n, texels: (0..n * n).map(|k| [k as f32, 0.0, 0.0]).collect() };
        for j in 0..n as i64 {
            let out = m.texel(n as i64, j);
            let (i2, j2) = ((out[0] as usize) % n, (out[0] as usize) / n);
            let a = OctMap::dir(n, n - 1, j as usize);
            let b = OctMap::dir(n, i2, j2);
            assert!(a.dot(b) > 0.95, "{j}: {a} {b}");
        }
    }

    #[test]
    fn white_furnace() {
        // a uniform environment stays uniform after prefiltering; its irradiance is π
        let spec = EnvSpec {
            zenith: [1.0; 3],
            horizon: [1.0; 3],
            floor_horizon: [1.0; 3],
            nadir: [1.0; 3],
            horizon_blur: 1.0,
            boxes: Vec::new(),
        };
        let m = EnvMaps::build(&spec);
        for r in [0.0, 0.25, 0.7, 1.0] {
            let s = m.specular(DVec3::new(0.3, 0.4, 0.5).normalize(), r);
            assert!((s[0] - 1.0).abs() < 0.02, "{r}: {s:?}");
        }
        let e = m.irradiance(DVec3::Z);
        assert!((e[0] - std::f64::consts::PI).abs() < 0.05, "{e:?}");
        assert!((up_irradiance(&spec) - 1.0).abs() < 1e-3);
        // DFG: a white metal reflects almost everything when smooth
        let (a, b) = Dfg::get().lookup(0.05, 0.9);
        assert!(a + b > 0.95 && a + b <= 1.001, "{a} {b}");
        let (a, b) = Dfg::get().lookup(1.0, 0.5);
        assert!(a + b < 0.85, "rough loses energy to multiple scattering: {}", a + b);
    }
}
