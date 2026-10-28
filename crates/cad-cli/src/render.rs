//! `mcad render`: the model as PNG.
//!
//! * Selection: the root (default), `--part NAME` (only these parts), `--line N` / `--node I`
//!   (highlighted as in the editor: nodes of the line full, the rest dimmed, zones blue,
//!   subtracted bodies red transparent; `--only` hides the rest).
//! * Camera: `--view`, `--azimuth/--elevation`, `--zoom`, `--target x,y,z`,
//!   `--fit all|part:NAME|line:N`, `--ortho`/`--perspective [--fov]`; `--views a,b,…|all`
//!   renders a contact sheet of labelled tiles with the same scale.
//! * Overlays `--show grids,anchors,handles,edges,labels,bbox,dims`.
//! * `--section x|y|z=WERT`, `--section-plane px,py,pz,nx,ny,nz`, `--section-flip`: clipping
//!   in the renderer with a filled cap (see `cad_render` and [`parse_section`] for the
//!   convention); `--xray`: everything transparent with edges.

use std::collections::HashMap;

use cad_i18n::{tr, trs};
use std::path::Path;
use std::rc::Rc;

use cad_eval::palette::{self, Theme};
use cad_kernel::Mesh;
use cad_render::{
    CameraInfo, Environment, Item, Label, Light, Marker, MarkerShape, Material, Polyline, Projection, RenderOptions,
    Scene, Section, Studio, StudioInfo, View, contact_sheet, render_info, tile_origin,
};
use cad_session::protocol as p;
use clap::Args;
use glam::{DMat4, DVec3};
use serde_json::{Value as Json, json};

use crate::{Common, Fail, Model, Outcome, load, mat, nodes_at_line, num, world_bounds};

/// Colours of a rendering in one theme (`--theme`); the light one is the CLI's classic look,
/// the dark one follows the UI's dark theme (`web/src/styles.css`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Look {
    pub theme: Theme,
    pub background: [f32; 4],
    pub part: [f32; 4],
    pub dim: [f32; 4],
    pub red: [f32; 4],
    pub blue: [f32; 4],
    pub grey: [f32; 4],
    pub edge: [f32; 4],
    pub dim_edge: [f32; 4],
    pub selected: [f32; 4],
    pub failed: [f32; 4],
    pub xray: [f32; 4],
    pub xray_hi: [f32; 4],
    pub xray_edge: [f32; 4],
    pub grid: [f32; 4],
    pub grid_point: [f32; 4],
    pub grid_label: [f32; 4],
    pub grid_point_label: [f32; 4],
    pub anchor: [f32; 4],
    pub handle: [f32; 4],
    pub handle_label: [f32; 4],
    pub bbox: [f32; 4],
    pub dims_label: [f32; 4],
    pub part_label: [f32; 4],
    pub hidden: [f32; 4],
    pub text: [f32; 4],
    pub cap: [f32; 4],
    pub label_background: [f32; 4],
    /// Keep-out volume (`assert_keepout`): fill and outline.
    pub keepout: [f32; 4],
    pub keepout_edge: [f32; 4],
    /// Intersection body of a violated assertion.
    pub overlap: [f32; 4],
    /// Contact face of `assert_touches`.
    pub contact: [f32; 4],
    /// Studio look: background gradient (top, bottom) and ground colour.
    pub studio_background: [[f32; 4]; 2],
    pub ground: [f32; 4],
}

const fn hex(c: u32, a: f32) -> [f32; 4] {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0, a]
}

pub(crate) const LIGHT: Look = Look {
    theme: Theme::Light,
    background: [0.96, 0.96, 0.97, 1.0],
    part: [0.75, 0.78, 0.84, 1.0],
    dim: [0.78, 0.79, 0.82, 0.22],
    red: [0.9, 0.2, 0.15, 0.35],
    blue: [0.2, 0.45, 0.95, 0.3],
    grey: [0.55, 0.55, 0.58, 0.3],
    edge: [0.15, 0.15, 0.18, 1.0],
    dim_edge: [0.45, 0.45, 0.5, 0.35],
    selected: [1.0, 0.55, 0.0, 1.0],
    failed: [0.95, 0.1, 0.1, 1.0],
    xray: [0.55, 0.66, 0.82, 0.22],
    xray_hi: [0.95, 0.62, 0.2, 0.42],
    xray_edge: [0.2, 0.25, 0.35, 0.9],
    grid: [0.22, 0.4, 0.78, 0.6],
    grid_point: [0.15, 0.3, 0.7, 0.95],
    grid_label: [0.1, 0.22, 0.6, 1.0],
    grid_point_label: [0.1, 0.18, 0.45, 1.0],
    anchor: [0.98, 0.76, 0.1, 1.0],
    handle: [1.0, 0.45, 0.05, 1.0],
    handle_label: [0.55, 0.22, 0.0, 1.0],
    bbox: [0.1, 0.55, 0.3, 0.9],
    dims_label: [0.0, 0.35, 0.12, 1.0],
    part_label: [0.1, 0.1, 0.45, 1.0],
    hidden: [0.15, 0.15, 0.3, 0.7],
    text: [0.08, 0.08, 0.12, 1.0],
    cap: [0.86, 0.36, 0.24, 1.0],
    label_background: [1.0, 1.0, 1.0, 0.78],
    keepout: hex(0xca8a04, 0.22),
    keepout_edge: hex(0x8a5a00, 1.0),
    overlap: hex(0xe03131, 0.92),
    contact: hex(0x2f9e44, 0.9),
    studio_background: [hex(0xf8f9fb, 1.0), hex(0xd9dde3, 1.0)],
    ground: hex(0xe6e8ec, 1.0),
};

pub(crate) const DARK: Look = Look {
    theme: Theme::Dark,
    background: hex(0x16191d, 1.0),
    part: hex(0x8f99a6, 1.0),
    dim: hex(0x5d6570, 0.22),
    red: hex(0xff5c63, 0.38),
    blue: hex(0x4c8dff, 0.32),
    grey: hex(0x8a909a, 0.3),
    edge: hex(0x0b0d10, 1.0),
    dim_edge: hex(0x7d8590, 0.4),
    selected: hex(0xffa94d, 1.0),
    failed: hex(0xff6b6b, 1.0),
    xray: hex(0x7d9cc4, 0.24),
    xray_hi: hex(0xffa94d, 0.45),
    xray_edge: hex(0xa9b8cc, 0.9),
    grid: hex(0xa78bfa, 0.65),
    grid_point: hex(0xc4b5fd, 0.95),
    grid_label: hex(0xc4b5fd, 1.0),
    grid_point_label: hex(0xb9a3ff, 1.0),
    anchor: hex(0xffc94d, 1.0),
    handle: hex(0xff922b, 1.0),
    handle_label: hex(0xffb86b, 1.0),
    bbox: hex(0x69db7c, 0.9),
    dims_label: hex(0x8ce99a, 1.0),
    part_label: hex(0x9cc4ff, 1.0),
    hidden: hex(0xa9b8cc, 0.6),
    text: hex(0xe4e7eb, 1.0),
    cap: hex(0xe0694b, 1.0),
    label_background: hex(0x1c1f23, 0.82),
    keepout: hex(0xfacc15, 0.2),
    keepout_edge: hex(0xfde047, 1.0),
    overlap: hex(0xff6b6b, 0.92),
    contact: hex(0x69db7c, 0.9),
    studio_background: [hex(0x2b3038, 1.0), hex(0x111317, 1.0)],
    ground: hex(0x2c3038, 1.0),
};

impl Look {
    /// Display colour of a model colour (palette colours adjusted to the theme).
    fn model(&self, c: [f32; 4]) -> [f32; 4] {
        let b = palette::rgba_bytes(c);
        let d = palette::display_rgb([b[0], b[1], b[2]], self.theme);
        [d[0] as f32 / 255.0, d[1] as f32 / 255.0, d[2] as f32 / 255.0, c[3]]
    }
}

/// Names of `--views all`.
pub(crate) const ALL_VIEWS: [&str; 4] = ["iso", "front", "top", "right"];
/// Grids with more points get no point labels (only plane names).
const MAX_POINT_LABELS: usize = 48;
const SHOW: [&str; 10] =
    ["grids", "anchors", "handles", "edges", "labels", "bbox", "dims", "regions", "faces", "asserts"];

/// Selection, camera, overlays, section (shared by `render` and `watch --render`).
#[derive(Args, Clone, Debug, Default)]
pub(crate) struct ViewArgs {
    /// Standard view: iso, front, back, top, bottom, left, right.
    #[arg(long, default_value = "iso")]
    pub view: String,
    /// Contact sheet: several views at the same scale, e.g. `iso,front,top,right`;
    /// `all` = iso,front,top,right.
    #[arg(long, value_name = "A,B,…|all")]
    pub views: Option<String>,
    /// Free camera: azimuth in degrees (0 = from the front/−y, 90 = from the right/+x).
    #[arg(long, allow_hyphen_values = true)]
    pub azimuth: Option<f64>,
    /// Free camera: elevation in degrees (90 = from above).
    #[arg(long, allow_hyphen_values = true)]
    pub elevation: Option<f64>,
    /// Magnification after fitting (2 = twice as large).
    #[arg(long)]
    pub zoom: Option<f64>,
    /// Look-at point `x,y,z` (context coordinates, mm).
    #[arg(long, value_name = "X,Y,Z", allow_hyphen_values = true)]
    pub target: Option<String>,
    /// Fit to `all` (whole model), `part:NAME` or `line:N` (default: everything drawn).
    #[arg(long, value_name = "all|part:NAME|line:N")]
    pub fit: Option<String>,
    /// Orthographic projection (default).
    #[arg(long, conflicts_with = "perspective")]
    pub ortho: bool,
    /// Perspective projection.
    #[arg(long)]
    pub perspective: bool,
    /// Field of view of the perspective projection in degrees.
    #[arg(long, default_value_t = 35.0)]
    pub fov: f64,
    /// Only this part (repeatable).
    #[arg(long = "part", value_name = "NAME")]
    pub part: Vec<String>,
    /// Nodes of this line in full, the rest dimmed (repeatable).
    #[arg(long, value_name = "N")]
    pub line: Vec<u32>,
    /// Node with this index (from `mcad eval`) in full, the rest dimmed (repeatable).
    #[arg(long, value_name = "I")]
    pub node: Vec<u32>,
    /// With --line/--node: leave the rest out instead of dimming it.
    #[arg(long)]
    pub only: bool,
    /// Image size WxH (default 1024x768, contact sheet 1600x1200 in total).
    #[arg(long, value_name = "WxH")]
    pub size: Option<String>,
    /// Keep the file context even if the line is inside a module definition.
    #[arg(long)]
    pub whole_file: bool,
    /// No axis triad.
    #[arg(long)]
    pub no_axes: bool,
    /// Overlays: grids, anchors, handles, edges, labels, bbox, dims (comma list, `all`).
    /// Colours: a model with color() shows the colour regions (body per colour, as printed);
    /// `faces`: colour per face from the nodes instead (faster, may differ from the print with
    /// overlaps and fillets); `regions` forces the regions.
    #[arg(long, value_delimiter = ',', value_name = "LIST")]
    pub show: Vec<String>,
    /// Section at an axis plane: `x=VALUE`, `y=VALUE`, `z=VALUE` (removes the +axis side).
    #[arg(long, value_name = "x|y|z=VALUE", allow_hyphen_values = true)]
    pub section: Option<String>,
    /// Section at any plane: point and normal; the side the normal points to is removed.
    #[arg(long, value_name = "PX,PY,PZ,NX,NY,NZ", allow_hyphen_values = true)]
    pub section_plane: Option<String>,
    /// Section: keep the other half (flip the normal).
    #[arg(long)]
    pub section_flip: bool,
    /// Everything semi-transparent with edges (x-ray).
    #[arg(long)]
    pub xray: bool,
    /// Colour scheme: light (default) or dark (like the dark theme of the UI).
    #[arg(long, default_value = "light", value_name = "light|dark")]
    pub theme: String,
    /// Rendering style: standard (CAD shading) or studio (lights, physically based materials
    /// with reflections, soft shadows, ambient occlusion, ground plane, gradient background).
    /// Default: standard (studio if the file has a render block).
    #[arg(long, value_name = "standard|studio")]
    pub style: Option<String>,
    /// Studio: key light direction in degrees: azimuth relative to the camera (0 = from the
    /// camera, positive = from the right), elevation above the xy plane (default -40,50).
    #[arg(long, value_name = "AZ,EL", allow_hyphen_values = true)]
    pub light: Option<String>,
    /// Studio: strength of key and fill light, 0…4 (default 1).
    #[arg(long, value_name = "F")]
    pub light_intensity: Option<f64>,
    /// Studio: share of the environment (sky) light, 0…1 (default 0.4).
    #[arg(long, value_name = "F")]
    pub ambient: Option<f64>,
    /// Studio: surroundings seen in reflections: studio (softboxes, default), overcast (soft
    /// sky only) or none (no reflections).
    #[arg(long, value_name = "studio|overcast|none")]
    pub environment: Option<String>,
    /// Studio: material of a colour or part (repeatable): `SEL=PRESET[,metalness=M,roughness=R,
    /// clearcoat=C]`; SEL is a colour (`red`, `#rrggbb`, `default` = uncoloured) or `part:NAME`;
    /// presets: plastic, plastic_glossy, rubber, aluminium, metal_brushed, chrome.
    #[arg(long = "material", value_name = "SEL=SPEC")]
    pub material: Vec<String>,
    /// Studio: cast shadows on the model and the ground (default on).
    #[arg(long, value_name = "on|off")]
    pub shadows: Option<String>,
    /// Studio: shadow edge softness, 0 = hard … 1 = very soft (default 0.35).
    #[arg(long, value_name = "F")]
    pub shadow_softness: Option<f64>,
    /// Studio: ambient occlusion in corners and gaps (default on).
    #[arg(long, value_name = "on|off")]
    pub ao: Option<String>,
    /// Studio: ambient occlusion radius in mm (default 5 % of the model diagonal).
    #[arg(long, value_name = "MM")]
    pub ao_radius: Option<f64>,
    /// Studio: ambient occlusion strength, 0…2 (default 1).
    #[arg(long, value_name = "F")]
    pub ao_strength: Option<f64>,
    /// Studio: ground plane under the model, fading into the background (default on).
    #[arg(long, value_name = "on|off")]
    pub ground: Option<String>,
    /// Studio: colour of the ground plane: `#rrggbb` or a palette name (default from the theme).
    #[arg(long, value_name = "COLOR")]
    pub ground_color: Option<String>,
    /// Studio: mesh edge lines of opaque bodies (default on; off: only the shading shows the
    /// form).
    #[arg(long, value_name = "on|off")]
    pub edges: Option<String>,
    /// Background: one colour, or two for a vertical gradient (top,bottom); `#rrggbb` or a
    /// palette name. Default: the theme's colour, with --style studio a light gradient.
    #[arg(long, value_name = "COLOR[,COLOR]")]
    pub background: Option<String>,
}

pub(crate) fn parse_size(s: &str) -> Result<(u32, u32), Fail> {
    let bad =
        || Fail::Usage(tr!("--size {s}: erwartet BxH, z. B. 1024x768", "--size {s}: expected WxH, e.g. 1024x768"));
    let (w, h) = s.split_once(['x', 'X']).ok_or_else(bad)?;
    let w: u32 = w.trim().parse().map_err(|_| bad())?;
    let h: u32 = h.trim().parse().map_err(|_| bad())?;
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return Err(bad());
    }
    Ok((w, h))
}

pub(crate) fn parse_vec(s: &str, n: usize, what: &str) -> Result<Vec<f64>, Fail> {
    let t = s.trim().trim_start_matches('[').trim_end_matches(']');
    let v: Result<Vec<f64>, _> = t.split(',').map(|x| x.trim().parse::<f64>()).collect();
    match v {
        Ok(v) if v.len() == n && v.iter().all(|x| x.is_finite()) => Ok(v),
        _ => Err(Fail::Usage(tr!(
            "{what} {s}: erwartet {n} Zahlen mit Komma",
            "{what} {s}: expected {n} comma-separated numbers"
        ))),
    }
}

/// Section plane from `--section x|y|z=WERT` / `--section-plane p,n` and `--section-flip`.
///
/// Convention (also for the UI): the plane through `point` with `normal` removes the half
/// space the normal points into; what stays is `(q − point) · normal ≤ 0`. `x=WERT` has the
/// normal +x (removes x > WERT), likewise y and z; `--section-flip` reverses the normal.
pub(crate) fn parse_section(sec: Option<&str>, plane: Option<&str>, flip: bool) -> Result<Option<Section>, Fail> {
    let s = match (sec, plane) {
        (Some(_), Some(_)) => {
            return Err(Fail::Usage(tr!(
                "--section und --section-plane schließen sich aus",
                "--section and --section-plane exclude each other"
            )));
        }
        (Some(s), None) => {
            let bad = || {
                Fail::Usage(tr!(
                    "--section {s}: erwartet x=WERT, y=WERT oder z=WERT",
                    "--section {s}: expected x=VALUE, y=VALUE or z=VALUE"
                ))
            };
            let (a, v) = s.split_once('=').ok_or_else(bad)?;
            let v: f64 = v.trim().parse().map_err(|_| bad())?;
            let n = match a.trim() {
                "x" => DVec3::X,
                "y" => DVec3::Y,
                "z" => DVec3::Z,
                _ => return Err(bad()),
            };
            Section { point: n * v, normal: n }
        }
        (None, Some(pl)) => {
            let v = parse_vec(pl, 6, "--section-plane")?;
            let n = DVec3::new(v[3], v[4], v[5]);
            if n.length() < 1e-12 {
                return Err(Fail::Usage(tr!(
                    "--section-plane: Normale ist null",
                    "--section-plane: the normal is zero"
                )));
            }
            Section { point: DVec3::new(v[0], v[1], v[2]), normal: n.normalize() }
        }
        (None, None) => {
            if flip {
                return Err(Fail::Usage(tr!(
                    "--section-flip braucht --section oder --section-plane",
                    "--section-flip needs --section or --section-plane"
                )));
            }
            return Ok(None);
        }
    };
    Ok(Some(if flip { Section { point: s.point, normal: -s.normal } } else { s }))
}

/// A parsed render request.
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub views: Vec<(String, View)>,
    pub sheet: bool,
    pub size: (u32, u32),
    pub projection: Projection,
    pub zoom: f64,
    pub target: Option<DVec3>,
    pub fit: Option<String>,
    pub parts: Vec<String>,
    pub lines: Vec<u32>,
    pub nodes: Vec<u32>,
    pub only: bool,
    pub axes: bool,
    pub show: Vec<String>,
    pub section: Option<Section>,
    pub xray: bool,
    pub look: &'static Look,
    /// `--style` and the studio options (resolved with the file's render block: [`Plan::resolve`]).
    pub studio_args: StudioArgs,
    /// `--background`: (top, bottom of a gradient).
    pub background: Option<Background>,
}

/// Which bodies a material applies to.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MatSel {
    /// Material of a colour (RGB bytes).
    Color([u8; 3]),
    /// Uncoloured material.
    Default,
    Part(String),
}

impl MatSel {
    /// `red`, `#rrggbb`, `default` or `part:NAME`.
    pub fn parse(s: &str) -> Option<MatSel> {
        let t = s.trim();
        if let Some(p) = t.strip_prefix("part:") {
            return (!p.trim().is_empty()).then(|| MatSel::Part(p.trim().to_string()));
        }
        if t.eq_ignore_ascii_case("default") {
            return Some(MatSel::Default);
        }
        let hexlike = (t.len() == 3 || t.len() == 6) && t.chars().all(|c| c.is_ascii_hexdigit());
        let c = if hexlike { palette::parse_color(&format!("#{t}")) } else { palette::parse_color(t) }?;
        if c.kind == palette::ColorKind::Random {
            return None;
        }
        let b = palette::rgba_bytes(c.rgba);
        Some(MatSel::Color([b[0], b[1], b[2]]))
    }

    fn text(&self) -> String {
        match self {
            MatSel::Color(b) => format!("#{:02x}{:02x}{:02x}", b[0], b[1], b[2]),
            MatSel::Default => "default".into(),
            MatSel::Part(p) => format!("part:{p}"),
        }
    }
}

/// A material entry: preset and single values (unset values: from the preset, else plastic).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct MatSpec {
    pub preset: Option<Material>,
    pub metalness: Option<f32>,
    pub roughness: Option<f32>,
    pub clearcoat: Option<f32>,
}

impl MatSpec {
    pub fn material(&self) -> Material {
        let b = self.preset.unwrap_or_default();
        Material {
            metalness: self.metalness.unwrap_or(b.metalness),
            roughness: self.roughness.unwrap_or(b.roughness),
            clearcoat: self.clearcoat.unwrap_or(b.clearcoat),
        }
        .clamped()
    }
}

/// The material of a body of colour `color` (`None`: uncoloured) in part `part`: the last
/// matching part entry, else the last matching colour entry, else plastic.
pub(crate) fn material_for(mats: &[(MatSel, Material)], color: Option<[f32; 4]>, part: Option<&str>) -> Material {
    // `feet` names all parts `feet[k]`
    let is = |n: &str, p: &str| n == p || p.strip_prefix(n).is_some_and(|r| r.starts_with('['));
    let by_part = part.and_then(|p| mats.iter().rev().find(|(s, _)| matches!(s, MatSel::Part(n) if is(n, p))));
    if let Some((_, m)) = by_part {
        return *m;
    }
    let key = color.map(|c| {
        let b = palette::rgba_bytes(c);
        [b[0], b[1], b[2]]
    });
    mats.iter()
        .rev()
        .find(|(s, _)| match (s, key) {
            (MatSel::Default, None) => true,
            (MatSel::Color(a), Some(b)) => *a == b,
            _ => false,
        })
        .map_or(Material::default(), |(_, m)| *m)
}

/// `--material SEL=SPEC`.
fn parse_material_arg(s: &str) -> Result<(MatSel, MatSpec), Fail> {
    let bad = |why: String| Fail::Usage(tr!("--material {s}: {why}", "--material {s}: {why}"));
    let (sel, spec) = s.split_once('=').ok_or_else(|| {
        bad(tr!("erwartet AUSWAHL=VOREINSTELLUNG[,roughness=R,…]", "expected SEL=PRESET[,roughness=R,…]"))
    })?;
    let sel = MatSel::parse(sel).ok_or_else(|| {
        bad(tr!("{sel}: erwartet eine Farbe, default oder part:NAME", "{sel}: expected a colour, default or part:NAME"))
    })?;
    let mut m = MatSpec::default();
    for tok in spec.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        match tok.split_once('=') {
            None => {
                m.preset = Some(Material::preset(tok).ok_or_else(|| {
                    bad(tr!(
                        "unbekanntes Material {tok} (bekannt: {})",
                        "unknown material {tok} (known: {})",
                        Material::preset_names()
                    ))
                })?);
            }
            Some((k, v)) => {
                let v: f64 = v.trim().parse().map_err(|_| bad(tr!("{k}: Zahl erwartet", "{k}: number expected")))?;
                let v = in_range(&format!("--material {}", k.trim()), v, 0.0, 1.0)? as f32;
                match k.trim() {
                    "metalness" => m.metalness = Some(v),
                    "roughness" => m.roughness = Some(v),
                    "clearcoat" => m.clearcoat = Some(v),
                    k => {
                        return Err(bad(tr!(
                            "unbekannter Wert {k} (metalness, roughness, clearcoat)",
                            "unknown value {k} (metalness, roughness, clearcoat)"
                        )));
                    }
                }
            }
        }
    }
    Ok((sel, m))
}

/// Writes the values of the file's render block over the defaults (the command line comes
/// after it).
fn apply_block(
    b: &p::RenderBlock,
    st: &mut Studio,
    materials: &mut Vec<(MatSel, Material)>,
    bg: &mut Option<Background>,
) {
    let color = |h: &str| palette::parse_color(h).map(|c| [c.rgba[0], c.rgba[1], c.rgba[2], 1.0]);
    if !b.lights.is_empty() {
        st.lights = b
            .lights
            .iter()
            .map(|l| Light {
                azimuth: l.azimuth,
                elevation: l.elevation,
                intensity: l.intensity,
                color: color(&l.color).map_or([1.0; 3], |c| [c[0], c[1], c[2]]),
            })
            .collect();
    }
    if let Some(v) = b.edges {
        st.edges = v;
    }
    if let Some(e) = b.environment.as_deref().and_then(Environment::from_name) {
        st.environment = e;
    }
    if let Some(v) = b.ambient {
        st.ambient = v;
    }
    if let Some(v) = b.light_intensity {
        st.light_intensity = v;
    }
    if let Some(v) = b.shadows {
        st.shadows = v;
    }
    if let Some(v) = b.shadow_softness {
        st.shadow_softness = v;
    }
    if let Some(v) = b.ao {
        st.ao = v;
    }
    if let Some(v) = b.ao_radius {
        st.ao_radius = Some(v);
    }
    if let Some(v) = b.ao_strength {
        st.ao_strength = v;
    }
    if let Some(v) = b.ground {
        st.ground = v;
    }
    if let Some(c) = b.ground_color.as_deref().and_then(color) {
        st.ground_color = c;
    }
    if let Some(v) = &b.background {
        let cs: Vec<[f32; 4]> = v.iter().filter_map(|h| color(h)).collect();
        *bg = match cs.as_slice() {
            [c] => Some((*c, None)),
            [a, b2] => Some((*a, Some(*b2))),
            _ => None,
        };
    }
    for m in &b.materials {
        let sel = match &m.target {
            p::RenderTarget::Part { part } => Some(MatSel::Part(part.clone())),
            p::RenderTarget::Color { color } => MatSel::parse(color),
        };
        let Some(sel) = sel else { continue };
        let spec = MatSpec {
            preset: m.preset.as_deref().and_then(Material::preset),
            metalness: m.metalness.map(|v| v as f32),
            roughness: m.roughness.map(|v| v as f32),
            clearcoat: m.clearcoat.map(|v| v as f32),
        };
        materials.push((sel, spec.material()));
    }
}

/// The studio options of the command line, checked (`None`: not given).
#[derive(Debug, Clone, Default)]
pub(crate) struct StudioArgs {
    /// `--style`: `Some(true)` = studio, `Some(false)` = standard.
    pub style: Option<bool>,
    pub light: Option<(f64, f64)>,
    pub light_intensity: Option<f64>,
    pub ambient: Option<f64>,
    pub environment: Option<Environment>,
    pub shadows: Option<bool>,
    pub shadow_softness: Option<f64>,
    pub ao: Option<bool>,
    pub ao_radius: Option<f64>,
    pub ao_strength: Option<f64>,
    pub ground: Option<bool>,
    pub ground_color: Option<[f32; 4]>,
    pub edges: Option<bool>,
    pub materials: Vec<(MatSel, MatSpec)>,
    /// Names of the studio options given (for "only applies with --style studio").
    pub given: Vec<&'static str>,
}

/// What a rendering uses after the command line and the file's render block are combined.
#[derive(Debug, Clone)]
pub(crate) struct Resolved {
    pub studio: Option<Studio>,
    /// Materials by colour or part, later entries win.
    pub materials: Vec<(MatSel, Material)>,
    pub background: Background,
    /// Line of the file's render block that was used.
    pub block_line: Option<u32>,
}

/// `on|off` of a studio switch.
fn parse_switch(opt: &str, v: &str) -> Result<bool, Fail> {
    match v.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "1" => Ok(true),
        "off" | "false" | "no" | "0" => Ok(false),
        _ => Err(Fail::Usage(tr!("{opt} {v}: erwartet on oder off", "{opt} {v}: expected on or off"))),
    }
}

/// A number option within `lo..=hi`.
fn in_range(opt: &str, v: f64, lo: f64, hi: f64) -> Result<f64, Fail> {
    if v.is_finite() && v >= lo && v <= hi {
        Ok(v)
    } else {
        Err(Fail::Usage(tr!("{opt} {v}: erlaubt {lo}…{hi}", "{opt} {v}: allowed {lo}…{hi}")))
    }
}

/// Colour of an option: `#rgb`, `#rrggbb` (also without `#`) or a palette name.
pub(crate) fn parse_color_arg(opt: &str, s: &str) -> Result<[f32; 4], Fail> {
    let t = s.trim();
    let hexlike = (t.len() == 3 || t.len() == 6) && t.chars().all(|c| c.is_ascii_hexdigit());
    let parsed = if hexlike { palette::parse_color(&format!("#{t}")) } else { palette::parse_color(t) };
    match parsed {
        Some(c) if c.kind != palette::ColorKind::Random => Ok([c.rgba[0], c.rgba[1], c.rgba[2], 1.0]),
        _ => Err(Fail::Usage(tr!(
            "{opt} {s}: erwartet #rrggbb oder einen Palettennamen (mcad usage colors)",
            "{opt} {s}: expected #rrggbb or a palette colour name (mcad usage colors)"
        ))),
    }
}

impl StudioArgs {
    /// The studio options of `a` with their values checked.
    pub fn from_args(a: &ViewArgs) -> Result<StudioArgs, Fail> {
        let style = match a.style.as_deref().map(|s| s.trim().to_ascii_lowercase()) {
            None => None,
            Some(s) if s == "standard" => Some(false),
            Some(s) if s == "studio" => Some(true),
            Some(_) => {
                return Err(Fail::Usage(tr!(
                    "--style {}: standard oder studio",
                    "--style {}: standard or studio",
                    a.style.as_deref().unwrap_or_default()
                )));
            }
        };
        let given: Vec<&'static str> = [
            ("--light", a.light.is_some()),
            ("--light-intensity", a.light_intensity.is_some()),
            ("--ambient", a.ambient.is_some()),
            ("--environment", a.environment.is_some()),
            ("--shadows", a.shadows.is_some()),
            ("--shadow-softness", a.shadow_softness.is_some()),
            ("--ao", a.ao.is_some()),
            ("--ao-radius", a.ao_radius.is_some()),
            ("--ao-strength", a.ao_strength.is_some()),
            ("--ground", a.ground.is_some()),
            ("--ground-color", a.ground_color.is_some()),
            ("--edges", a.edges.is_some()),
            ("--material", !a.material.is_empty()),
        ]
        .into_iter()
        .filter(|(_, g)| *g)
        .map(|(n, _)| n)
        .collect();
        let mut s = StudioArgs { style, given, ..StudioArgs::default() };
        if let Some(l) = &a.light {
            let v = parse_vec(l, 2, "--light")?;
            s.light = Some((v[0], in_range("--light (elevation)", v[1], 1.0, 90.0)?));
        }
        if let Some(v) = a.light_intensity {
            s.light_intensity = Some(in_range("--light-intensity", v, 0.0, 4.0)?);
        }
        if let Some(v) = a.ambient {
            s.ambient = Some(in_range("--ambient", v, 0.0, 1.0)?);
        }
        if let Some(v) = &a.environment {
            s.environment = Some(Environment::from_name(v).ok_or_else(|| {
                Fail::Usage(tr!(
                    "--environment {v}: erwartet {}",
                    "--environment {v}: expected {}",
                    Environment::NAMES.join(", ")
                ))
            })?);
        }
        if let Some(v) = &a.shadows {
            s.shadows = Some(parse_switch("--shadows", v)?);
        }
        if let Some(v) = a.shadow_softness {
            s.shadow_softness = Some(in_range("--shadow-softness", v, 0.0, 1.0)?);
        }
        if let Some(v) = &a.ao {
            s.ao = Some(parse_switch("--ao", v)?);
        }
        if let Some(v) = a.ao_radius {
            if !(v.is_finite() && v > 0.0) {
                return Err(Fail::Usage(tr!(
                    "--ao-radius {v}: muss positiv sein",
                    "--ao-radius {v}: must be positive"
                )));
            }
            s.ao_radius = Some(v);
        }
        if let Some(v) = a.ao_strength {
            s.ao_strength = Some(in_range("--ao-strength", v, 0.0, 2.0)?);
        }
        if let Some(v) = &a.ground {
            s.ground = Some(parse_switch("--ground", v)?);
        }
        if let Some(c) = &a.ground_color {
            s.ground_color = Some(parse_color_arg("--ground-color", c)?);
        }
        if let Some(v) = &a.edges {
            s.edges = Some(parse_switch("--edges", v)?);
        }
        for m in &a.material {
            s.materials.push(parse_material_arg(m)?);
        }
        Ok(s)
    }

    /// Writes the given options over `st`.
    fn apply(&self, st: &mut Studio) {
        if let Some((az, el)) = self.light {
            match st.lights.first_mut() {
                Some(l) => (l.azimuth, l.elevation) = (az, el),
                None => st.lights.push(Light::new(az, el)),
            }
        }
        if let Some(v) = self.light_intensity {
            st.light_intensity = v;
        }
        if let Some(v) = self.ambient {
            st.ambient = v;
        }
        if let Some(v) = self.environment {
            st.environment = v;
        }
        if let Some(v) = self.shadows {
            st.shadows = v;
        }
        if let Some(v) = self.shadow_softness {
            st.shadow_softness = v;
        }
        if let Some(v) = self.ao {
            st.ao = v;
        }
        if let Some(v) = self.ao_radius {
            st.ao_radius = Some(v);
        }
        if let Some(v) = self.ao_strength {
            st.ao_strength = v;
        }
        if let Some(v) = self.ground {
            st.ground = v;
        }
        if let Some(v) = self.ground_color {
            st.ground_color = v;
        }
        if let Some(v) = self.edges {
            st.edges = v;
        }
    }
}

/// Background colour, with the bottom colour of a vertical gradient.
pub(crate) type Background = ([f32; 4], Option<[f32; 4]>);

/// `--background C` or `C1,C2`.
fn background_from_args(a: &ViewArgs) -> Result<Option<Background>, Fail> {
    let Some(b) = &a.background else { return Ok(None) };
    let parts: Vec<&str> = b.split(',').map(str::trim).collect();
    match parts.as_slice() {
        [c] => Ok(Some((parse_color_arg("--background", c)?, None))),
        [c1, c2] => Ok(Some((parse_color_arg("--background", c1)?, Some(parse_color_arg("--background", c2)?)))),
        _ => Err(Fail::Usage(tr!(
            "--background {b}: erwartet FARBE oder OBEN,UNTEN",
            "--background {b}: expected COLOR or TOP,BOTTOM"
        ))),
    }
}

impl Plan {
    pub fn from_args(a: &ViewArgs) -> Result<Plan, Fail> {
        let one = |name: &str| {
            View::from_name(name).ok_or_else(|| {
                Fail::Usage(tr!(
                    "--view {name}: iso, front, back, top, bottom, left oder right",
                    "--view {name}: iso, front, back, top, bottom, left or right"
                ))
            })
        };
        let (views, sheet) = match &a.views {
            Some(v) => {
                let names: Vec<String> = if v.trim() == "all" {
                    ALL_VIEWS.iter().map(|s| s.to_string()).collect()
                } else {
                    v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
                };
                if names.is_empty() {
                    return Err(Fail::Usage(tr!("--views: keine Ansicht", "--views: no view")));
                }
                let mut out = Vec::new();
                for n in names {
                    let view = one(&n).map_err(|_| {
                        Fail::Usage(tr!("--views: unbekannte Ansicht {n}", "--views: unknown view {n}"))
                    })?;
                    out.push((n, view));
                }
                (out, true)
            }
            None => {
                let base = one(&a.view)?;
                if a.azimuth.is_some() || a.elevation.is_some() {
                    let (az, el) = base.angles();
                    let (az, el) = (a.azimuth.unwrap_or(az), a.elevation.unwrap_or(el));
                    (vec![(format!("az {} el {}", num(az), num(el)), View::from_angles(az, el))], false)
                } else {
                    (vec![(a.view.clone(), base)], false)
                }
            }
        };
        let size = match &a.size {
            Some(s) => parse_size(s)?,
            None if sheet => (1600, 1200),
            None => (1024, 768),
        };
        let mut show: Vec<String> = Vec::new();
        for s in &a.show {
            let s = s.trim();
            if s == "all" {
                show.extend(SHOW.iter().map(|x| x.to_string()));
            } else if SHOW.contains(&s) {
                show.push(s.to_string());
            } else if !s.is_empty() {
                return Err(Fail::Usage(tr!("--show {s}: erwartet {}", "--show {s}: expected {}", SHOW.join(", "))));
            }
        }
        show.dedup();
        if let Some(z) = a.zoom
            && !(z.is_finite() && z > 0.0)
        {
            return Err(Fail::Usage(tr!("--zoom {z}: muss positiv sein", "--zoom {z}: must be positive")));
        }
        if !(a.fov > 1.0 && a.fov < 170.0) {
            return Err(Fail::Usage(tr!("--fov {}: 1…170 Grad", "--fov {}: 1…170 degrees", a.fov)));
        }
        let target = match &a.target {
            Some(t) => {
                let v = parse_vec(t, 3, "--target")?;
                Some(DVec3::new(v[0], v[1], v[2]))
            }
            None => None,
        };
        let look: &'static Look = match a.theme.trim().to_ascii_lowercase().as_str() {
            "light" | "hell" | "" => &LIGHT,
            "dark" | "dunkel" => &DARK,
            t => return Err(Fail::Usage(tr!("--theme {t}: light oder dark", "--theme {t}: light or dark"))),
        };
        Ok(Plan {
            views,
            sheet,
            size,
            projection: if a.perspective { Projection::Perspective { fov_deg: a.fov } } else { Projection::Ortho },
            zoom: a.zoom.unwrap_or(1.0),
            target,
            fit: a.fit.clone(),
            parts: a.part.clone(),
            lines: a.line.clone(),
            nodes: a.node.clone(),
            only: a.only,
            axes: !a.no_axes,
            show,
            section: parse_section(a.section.as_deref(), a.section_plane.as_deref(), a.section_flip)?,
            xray: a.xray,
            look,
            studio_args: StudioArgs::from_args(a)?,
            background: background_from_args(a)?,
        })
    }

    /// Style, studio settings, materials and background from the command line over the file's
    /// render block (`block`); exit 2 for studio options without the studio style.
    pub fn resolve(&self, block: Option<&p::RenderBlock>) -> Result<Resolved, Fail> {
        let a = &self.studio_args;
        // a render block makes the studio look the default (unless it says `style = "standard"`)
        let studio_on = a.style.or(block.map(|b| b.style.as_deref() != Some("standard"))).unwrap_or(false);
        if !studio_on {
            if let Some(first) = a.given.first() {
                return Err(Fail::Usage(tr!(
                    "{first} gilt nur mit --style studio",
                    "{first} only applies with --style studio"
                )));
            }
            let background = self.background.unwrap_or((self.look.background, None));
            return Ok(Resolved { studio: None, materials: Vec::new(), background, block_line: None });
        }
        let mut st = Studio { ground_color: self.look.ground, ..Studio::default() };
        let mut materials: Vec<(MatSel, Material)> = Vec::new();
        let mut background: Option<Background> = None;
        if let Some(b) = block {
            apply_block(b, &mut st, &mut materials, &mut background);
        }
        a.apply(&mut st);
        materials.extend(a.materials.iter().map(|(s, m)| (s.clone(), m.material())));
        let background = self
            .background
            .or(background)
            .unwrap_or((self.look.studio_background[0], Some(self.look.studio_background[1])));
        Ok(Resolved { studio: Some(st), materials, background, block_line: block.map(|b| b.range.line) })
    }

    fn shows(&self, s: &str) -> bool {
        self.show.iter().any(|x| x == s)
    }
}

/// `true` if `n` or an ancestor satisfies `f`.
fn up(sc: &p::Scene, mut n: u32, f: impl Fn(&p::SceneNode) -> bool) -> bool {
    loop {
        let node = &sc.nodes[n as usize];
        if f(node) {
            return true;
        }
        match node.parent {
            Some(par) if Some(n) != sc.root => n = par,
            _ => return false,
        }
    }
}

struct Draw {
    node: u32,
    color: [f32; 4],
    edges: Option<[f32; 4]>,
    /// Split the mesh by `color()` of the creating nodes.
    node_colors: bool,
    role: &'static str,
}

/// Colour per triangle of the mesh of node `owner` (its own system), from `color()` of the
/// node that created the face (Bericht 12, F1): a face tag belongs to all nodes of equal
/// content (the cache shares their result), so only those below `owner` count, and of several
/// with different colours the one whose bounds contain the triangle (the latest in pre-order
/// if several do). Approximate where colours overlap or a fillet joins them; the regions
/// (`regions_on`) are exact.
pub(crate) fn face_colors(sc: &p::Scene, owner: u32, m: &Mesh) -> Vec<Option<[f32; 4]>> {
    let below = |mut n: u32| loop {
        if n == owner {
            return true;
        }
        match sc.nodes[n as usize].parent {
            Some(par) => n = par,
            None => return false,
        }
    };
    let mut cands: HashMap<u32, Vec<u32>> = HashMap::new();
    for tn in &sc.tag_nodes {
        let v: Vec<u32> = tn.nodes.iter().copied().filter(|&n| below(n)).collect();
        if !v.is_empty() {
            cands.insert(tn.tag, v);
        }
    }
    let ow = mat(&sc.nodes[owner as usize].display_world);
    let mut inv: HashMap<u32, DMat4> = HashMap::new();
    let tris = m.indices.len() / 3;
    let mut out = Vec::with_capacity(tris);
    for t in 0..tris {
        let Some(c) = cands.get(&m.tri_tags[t]) else {
            out.push(None);
            continue;
        };
        let col = |n: u32| sc.nodes[n as usize].color.map(|c| c.0);
        let first = col(c[0]);
        if c.iter().all(|&n| col(n) == first) {
            out.push(first);
            continue;
        }
        let mut p = DVec3::ZERO;
        for k in 0..3 {
            let v = m.indices[3 * t + k] as usize * 3;
            p += DVec3::new(m.positions[v] as f64, m.positions[v + 1] as f64, m.positions[v + 2] as f64) / 3.0;
        }
        let pw = ow.transform_point3(p);
        let mut pick = None;
        for &n in c {
            let node = &sc.nodes[n as usize];
            let Some(b) = &node.bounds else { continue };
            let iv = *inv.entry(n).or_insert_with(|| mat(&node.display_world).inverse());
            let q = iv.transform_point3(pw);
            let (lo, hi) = (DVec3::from_array(b.min.0), DVec3::from_array(b.max.0));
            let eps = 1e-6 * (hi - lo).length() + 1e-4;
            if q.cmpge(lo - eps).all() && q.cmple(hi + eps).all() {
                pick = Some(pick.map_or(n, |x: u32| x.max(n)));
            }
        }
        out.push(col(pick.unwrap_or(c[0])));
    }
    out
}

/// Splits a mesh by triangle colour (`None`: `base`) and studio material. Edges stay with the
/// first part.
fn split_by_color(
    m: &Mesh,
    tri_colors: &[Option<[f32; 4]>],
    tri_materials: &[Material],
    base: [f32; 4],
) -> Vec<(Mesh, [f32; 4], Material)> {
    let colors: Vec<([f32; 4], Material)> =
        tri_colors.iter().zip(tri_materials).map(|(c, mt)| (c.unwrap_or(base), *mt)).collect();
    if colors.iter().all(|c| *c == colors[0] && c.0 == base) {
        return vec![(m.clone(), base, colors.first().map_or(Material::default(), |c| c.1))];
    }
    // (colour, material) → triangles
    type Group = (([f32; 4], Material), Vec<usize>);
    let mut groups: Vec<Group> = Vec::new();
    for (i, c) in colors.iter().enumerate() {
        match groups.iter_mut().find(|(gc, _)| gc == c) {
            Some(g) => g.1.push(i),
            None => groups.push((*c, vec![i])),
        }
    }
    let mut out = Vec::new();
    for (k, ((c, mt), tris)) in groups.into_iter().enumerate() {
        let mut sub = Mesh::default();
        let mut remap: HashMap<u32, u32> = HashMap::new();
        for t in tris {
            for &vi in &m.indices[3 * t..3 * t + 3] {
                let ni = *remap.entry(vi).or_insert_with(|| {
                    let v = vi as usize * 3;
                    sub.positions.extend_from_slice(&m.positions[v..v + 3]);
                    sub.normals.extend_from_slice(&m.normals[v..v + 3]);
                    (sub.positions.len() / 3 - 1) as u32
                });
                sub.indices.push(ni);
            }
            sub.tri_tags.push(m.tri_tags[t]);
        }
        if k == 0 {
            sub.edge_segments = m.edge_segments.clone();
            sub.edge_ids = m.edge_ids.clone();
        }
        let mut c = c;
        c[3] = base[3];
        out.push((sub, c, mt));
    }
    out
}

fn polyline(points: &[p::Vec3], m: &DMat4, color: [f32; 4], width: f32) -> Polyline {
    Polyline::new(points.iter().map(|q| m.transform_point3(DVec3::from_array(q.0))).collect(), color, width)
}

fn top_line(points: Vec<DVec3>, color: [f32; 4], width: f32, dashed: bool) -> Polyline {
    Polyline { points, color, width, on_top: true, dashed }
}

/// Bounds union.
pub(crate) fn union(a: Option<p::Bounds>, b: &p::Bounds) -> Option<p::Bounds> {
    Some(match a {
        None => b.clone(),
        Some(a) => p::Bounds {
            min: p::Vec3(DVec3::from_array(a.min.0).min(DVec3::from_array(b.min.0)).to_array()),
            max: p::Vec3(DVec3::from_array(a.max.0).max(DVec3::from_array(b.max.0)).to_array()),
        },
    })
}

/// World bounds of nodes (context coordinates).
pub(crate) fn nodes_bounds(sc: &p::Scene, nodes: &[u32]) -> Option<p::Bounds> {
    let mut acc = None;
    for &n in nodes {
        let node = &sc.nodes[n as usize];
        if node.status == p::NodeStatus::Skipped {
            continue;
        }
        if let Some(b) = &node.bounds {
            acc = union(acc, &world_bounds(b, &mat(&node.display_world)));
        }
    }
    acc
}

pub(crate) fn bounds_json(b: &Option<p::Bounds>) -> Json {
    match b {
        None => Json::Null,
        Some(b) => {
            let r = |v: f64| {
                let x = (v * 1e9).round() / 1e9;
                if x == 0.0 { 0.0 } else { x }
            };
            let (lo, hi): (Vec<f64>, Vec<f64>) = (0..3).map(|i| (r(b.min.0[i]), r(b.max.0[i]))).unzip();
            let s: Vec<f64> = (0..3).map(|i| r(hi[i] - lo[i])).collect();
            json!({ "min": lo, "max": hi, "size": s })
        }
    }
}

/// Nodes of a target spec `all`, `part:NAME` (or a bare part name), `line:N`, `node:I`.
pub(crate) fn target_nodes(sc: &p::Scene, spec: &str) -> Result<Vec<u32>, Fail> {
    let spec = spec.trim();
    if spec == "all" {
        return Ok(sc.root.into_iter().collect());
    }
    if let Some(l) = spec.strip_prefix("line:") {
        let l: u32 = l
            .trim()
            .parse()
            .map_err(|_| Fail::Usage(tr!("{spec}: erwartet line:ZEILE", "{spec}: expected line:LINE")))?;
        let n = nodes_at_line(sc, l);
        if n.is_empty() {
            return Err(Fail::Usage(tr!(
                "{spec}: Zeile {l} erzeugt keine Geometrie",
                "{spec}: line {l} produces no geometry"
            )));
        }
        return Ok(n);
    }
    if let Some(i) = spec.strip_prefix("node:") {
        let i: u32 = i
            .trim()
            .parse()
            .map_err(|_| Fail::Usage(tr!("{spec}: erwartet node:INDEX", "{spec}: expected node:INDEX")))?;
        if i as usize >= sc.nodes.len() {
            return Err(Fail::Usage(tr!(
                "{spec}: Knoten {i} existiert nicht ({} Knoten)",
                "{spec}: node {i} does not exist ({} nodes)",
                sc.nodes.len()
            )));
        }
        return Ok(vec![i]);
    }
    let name = spec.strip_prefix("part:").unwrap_or(spec);
    match sc.parts.iter().find(|pt| pt.name == name) {
        Some(pt) => Ok(vec![pt.node]),
        None => {
            let known: Vec<&str> = sc.parts.iter().map(|p| p.name.as_str()).collect();
            Err(Fail::Usage(tr!(
                "Bauteil `{name}` nicht gefunden (Bauteile: {}; Ziele: all, part:NAME, line:N, node:I)",
                "part `{name}` not found (parts: {}; targets: all, part:NAME, line:N, node:I)",
                if known.is_empty() { trs!("keine", "none").to_string() } else { known.join(", ") }
            )))
        }
    }
}

/// (mesh, colour, placement, edge colour, section cap)
/// Mesh, colour, transform, edge colour, cap, studio material.
type Owned = (Mesh, [f32; 4], DMat4, Option<[f32; 4]>, bool, Material);

/// Everything to draw, built once per evaluation (meshes owned).
struct Built {
    meshes: Vec<Owned>,
    lines: Vec<Polyline>,
    markers: Vec<Marker>,
    labels: Vec<Label>,
    triangles: usize,
    visible: Vec<Json>,
    highlighted: Vec<u32>,
    subject: Option<p::Bounds>,
    counts: Json,
    notes: Vec<String>,
}

fn build(m: &mut Model, sc: &p::Scene, plan: &Plan, res: &Resolved) -> Result<Built, Fail> {
    let live = |n: u32| sc.nodes[n as usize].status != p::NodeStatus::Skipped;
    let mut highlighted: Vec<u32> = Vec::new();
    for &l in &plan.lines {
        for n in nodes_at_line(sc, l) {
            if !highlighted.contains(&n) {
                highlighted.push(n);
            }
        }
    }
    for &n in &plan.nodes {
        if n as usize >= sc.nodes.len() {
            return Err(Fail::Usage(tr!(
                "--node {n}: Knoten existiert nicht ({} Knoten)",
                "--node {n}: node does not exist ({} nodes)",
                sc.nodes.len()
            )));
        }
        if !highlighted.contains(&n) {
            highlighted.push(n);
        }
    }
    let mut base: Vec<u32> = Vec::new();
    for name in &plan.parts {
        base.extend(target_nodes(sc, &format!("part:{name}"))?);
    }
    if plan.parts.is_empty() {
        base.extend(section_parts(sc, plan).unwrap_or_else(|| sc.root.into_iter().collect()));
    }
    base.retain(|&n| live(n));
    let selecting = !plan.lines.is_empty() || !plan.nodes.is_empty();

    let lk = plan.look;
    let mut draws: Vec<Draw> = Vec::new();
    let (part_c, dim_c) = if plan.xray { (lk.xray, lk.xray) } else { (lk.part, lk.dim) };
    let edge_c = if plan.xray { Some(lk.xray_edge) } else { Some(lk.edge) };
    for &b in &base {
        if !selecting || highlighted.contains(&b) {
            draws.push(Draw { node: b, color: part_c, edges: edge_c, node_colors: !plan.xray, role: "base" });
        } else if !plan.only {
            let e = if plan.xray { edge_c } else { Some(lk.dim_edge) };
            draws.push(Draw { node: b, color: dim_c, edges: e, node_colors: false, role: "dimmed" });
        }
    }
    for &h in &highlighted {
        if !live(h) || base.contains(&h) {
            continue;
        }
        let n = &sc.nodes[h as usize];
        let (color, edges, role) = if up(sc, h, |x| x.flags.zone) {
            (lk.blue, None, "zone")
        } else if up(sc, h, |x| x.subtracted) {
            (lk.red, None, "subtracted")
        } else if up(sc, h, |x| x.flags.background) {
            (lk.grey, None, "background")
        } else if plan.xray {
            (lk.xray_hi, edge_c, "highlighted")
        } else {
            (lk.part, Some(lk.edge), "highlighted")
        };
        draws.push(Draw { node: h, color, edges, node_colors: color == lk.part, role });
        // Display of the operation: subtracted bodies red, zones blue.
        for &ch in &n.children {
            let cn = &sc.nodes[ch as usize];
            if !live(ch) {
                continue;
            }
            if cn.subtracted {
                draws.push(Draw { node: ch, color: lk.red, edges: None, node_colors: false, role: "subtracted" });
            } else if cn.flags.zone
                && matches!(n.label.as_str(), "fillet" | "chamfer" | "shell" | "fillet2d" | "chamfer2d")
            {
                draws.push(Draw { node: ch, color: lk.blue, edges: None, node_colors: false, role: "zone" });
            }
        }
    }
    let mut lines: Vec<Polyline> = Vec::new();
    for &h in &highlighted {
        let n = &sc.nodes[h as usize];
        let w = mat(&n.display_world);
        for e in &n.selected_edges {
            lines.push(polyline(e, &w, lk.selected, 2.5));
        }
        // 2D operations (Plan 17.3): input contour, offset contour dashed, stroke centre line
        for g in &n.guides {
            let pts: Vec<DVec3> = g.points.iter().map(|q| w.transform_point3(DVec3::from_array(q.0))).collect();
            match g.kind.as_str() {
                "result" => lines.push(top_line(pts, lk.selected, 2.0, true)),
                "centerline" => lines.push(top_line(pts, lk.handle, 1.6, true)),
                "source" => lines.push(top_line(pts, lk.edge, 1.4, false)),
                _ => {}
            }
        }
        // (places of failed Booleans: `failure_overlay`, always)
        for msg in n.messages.iter().filter(|m| m.at.is_none()) {
            for e in &msg.edges {
                lines.push(polyline(e, &w, lk.failed, 3.0));
            }
        }
    }
    // Modifiers are always visible: `#` red transparent, `%` grey transparent.
    if plan.parts.is_empty() {
        for n in &sc.nodes {
            if !live(n.idx) || n.flags.disabled {
                continue;
            }
            let parent_has = |f: &dyn Fn(&p::SceneNode) -> bool| n.parent.is_some_and(|par| up(sc, par, |x| f(x)));
            if n.flags.highlight && !parent_has(&|x| x.flags.highlight) {
                draws.push(Draw { node: n.idx, color: lk.red, edges: None, node_colors: false, role: "modifier #" });
            }
            if n.flags.keepout && !parent_has(&|x| x.flags.keepout) {
                draws.push(Draw { node: n.idx, color: lk.keepout, edges: None, node_colors: false, role: "keepout" });
            } else if n.flags.background && !parent_has(&|x| x.flags.background) {
                draws.push(Draw { node: n.idx, color: lk.grey, edges: None, node_colors: false, role: "modifier %" });
            }
        }
    }

    // `regions` (default for a model with colours, Bericht 12, F1/F3): bodies per colour as
    // the 3MF prints them, instead of colours per face (the base draws)
    let colored = sc.colors.iter().any(|c| c.source != p::ColorSource::Default);
    let has_base = draws.iter().any(|d| d.role == "base");
    // studio materials of parts: the regions are bodies per part
    let part_materials = res.studio.is_some() && res.materials.iter().any(|(s, _)| matches!(s, MatSel::Part(_)));
    let regions_on = !plan.xray
        && has_base
        && (plan.shows("regions") || ((colored || part_materials) && sc.complete && !plan.shows("faces")));
    let mut region_meshes: Vec<(u32, Mesh, [f32; 4], Material)> = Vec::new();
    if regions_on {
        let sets = m.session.color_regions(true, false).map_err(Fail::Kernel)?;
        let info_of = |key: &str| sc.colors.iter().find(|c| c.key == key);
        let color_of = |key: &str| info_of(key).map(|c| lk.model(c.rgba.0));
        for set in sets {
            if !plan.parts.is_empty() && !set.part.as_ref().is_some_and(|n| plan.parts.contains(n)) {
                continue;
            }
            for r in set.regions {
                let Some(md) = r.mesh else { continue };
                let mesh = Mesh {
                    positions: md.positions,
                    normals: md.normals,
                    indices: md.indices,
                    tri_tags: md.tri_tags,
                    tri_faces: md.tri_faces,
                    edge_segments: md.edge_segments,
                    edge_ids: md.edge_ids,
                };
                let raw = info_of(&r.color).filter(|c| c.source != p::ColorSource::Default).map(|c| c.rgba.0);
                let mt = material_for(&res.materials, raw, set.part.as_deref());
                region_meshes.push((set.node, mesh, color_of(&r.color).unwrap_or(lk.part), mt));
            }
        }
    }
    let mut meshes = Vec::new();
    let mut triangles = 0usize;
    let mut visible = Vec::new();
    let mut keepout_lines: Vec<Polyline> = Vec::new();
    for (node, mesh, col, mt) in &region_meshes {
        let n = &sc.nodes[*node as usize];
        triangles += mesh.triangle_count();
        visible.push(json!({
            "idx": n.idx, "label": n.label, "instance": n.instance, "role": "region",
            "line": n.range.line, "file": n.range.file, "triangles": mesh.triangle_count(),
        }));
        meshes.push((mesh.clone(), *col, mat(&n.display_world), Some(lk.edge), true, *mt));
    }
    for d in &draws {
        if regions_on && d.role == "base" {
            continue;
        }
        let mesh: Rc<Mesh> = m.session.mesh(d.node, false).map_err(Fail::Kernel)?;
        let node = &sc.nodes[d.node as usize];
        let w = mat(&node.display_world);
        triangles += mesh.triangle_count();
        let cap = d.color[3] >= 0.999;
        if d.node_colors {
            let fc = face_colors(sc, d.node, &mesh);
            let mts: Vec<Material> = fc.iter().map(|c| material_for(&res.materials, *c, None)).collect();
            let fc: Vec<Option<[f32; 4]>> = fc.into_iter().map(|c| c.map(|c| lk.model(c))).collect();
            for (sub, col, mt) in split_by_color(&mesh, &fc, &mts, d.color) {
                meshes.push((sub, col, w, d.edges, cap, mt));
            }
        } else {
            meshes.push(((*mesh).clone(), d.color, w, d.edges, cap, material_for(&res.materials, None, None)));
        }
        if d.role == "keepout" {
            // outline on top, dashed: visible inside the model
            for seg in mesh.edge_segments.chunks_exact(6) {
                let a = w.transform_point3(DVec3::new(seg[0] as f64, seg[1] as f64, seg[2] as f64));
                let b = w.transform_point3(DVec3::new(seg[3] as f64, seg[4] as f64, seg[5] as f64));
                keepout_lines.push(top_line(vec![a, b], lk.keepout_edge, 1.6, true));
            }
        }
        visible.push(json!({
            "idx": node.idx, "label": node.label, "instance": node.instance, "role": d.role,
            "line": node.range.line, "file": node.range.file, "triangles": mesh.triangle_count(),
        }));
    }

    let subject_nodes: Vec<u32> = if highlighted.is_empty() { base.clone() } else { highlighted.clone() };
    let subject = nodes_bounds(sc, &subject_nodes).or_else(|| nodes_bounds(sc, &base));
    let mut markers = Vec::new();
    let mut labels = Vec::new();
    let mut notes = Vec::new();
    let size = subject
        .as_ref()
        .map(|b| (DVec3::from_array(b.max.0) - DVec3::from_array(b.min.0)).length())
        .filter(|s| *s > 1e-9)
        .unwrap_or(10.0);
    let labels_on = plan.shows("labels");
    let (mut n_grids, mut n_points, mut n_anchors, mut n_handles) = (0, 0, 0, 0);

    if plan.shows("grids") {
        let grids: Vec<&p::Grid> = sc.grids.iter().filter(|g| g.in_context).collect();
        if grids.is_empty() {
            notes.push(tr!(
                "keine Raster im Kontext (Raster gelten nur in ihrem Modul: --module NAME)",
                "no grids in the context (grids hold only in their module: --module NAME)"
            ));
        }
        for g in grids {
            let n: usize = (0..3u8).map(|a| g.planes.iter().filter(|pl| pl.axis == a).count().max(1)).product();
            if labels_on && n > MAX_POINT_LABELS {
                notes.push(tr!(
                    "Raster {}: {n} Punkte, nur Ebenen beschriftet (Punkte bis {MAX_POINT_LABELS}; --view und --zoom für Details)",
                    "grid {}: {n} points, only planes labelled (points up to {MAX_POINT_LABELS}; --view and --zoom for details)",
                    g.name
                ));
            }
            n_grids += 1;
            let frame = profile_grid_frame(sc, g);
            if frame.is_some() {
                notes.push(tr!(
                    "Raster {}: in der Profilebene von rotate_extrude gezeichnet (x = Radius, y = Höhe)",
                    "grid {}: drawn in the profile plane of rotate_extrude (x = radius, y = height)",
                    g.name
                ));
            }
            n_points +=
                grid_overlay(g, frame, subject.as_ref(), size, labels_on, lk, (&mut lines, &mut markers, &mut labels));
        }
    }
    if plan.shows("anchors") {
        for a in &sc.anchors {
            n_anchors += 1;
            let pnt = DVec3::from_array(a.point.0);
            markers.push(Marker { pos: pnt, shape: MarkerShape::Diamond, size: 5.5, color: lk.anchor, on_top: true });
            let name = a.text.clone().unwrap_or_else(|| a.name.clone());
            labels.push(Label { pos: pnt, text: name, color: lk.text, offset: (7.0, -18.0), priority: 4 });
            if let Some(d) = &a.dir {
                let d = DVec3::from_array(d.0).normalize_or_zero();
                if d != DVec3::ZERO {
                    let l = size * 0.12;
                    let tip = pnt + d * l;
                    let side = d.any_orthonormal_vector();
                    lines.push(top_line(vec![pnt, tip], lk.anchor, 2.0, false));
                    lines.push(top_line(
                        vec![tip - d * l * 0.3 + side * l * 0.15, tip, tip - d * l * 0.3 - side * l * 0.15],
                        lk.anchor,
                        2.0,
                        false,
                    ));
                }
            }
        }
    }
    if plan.shows("handles") {
        for h in &sc.handles {
            n_handles += 1;
            let pnt = DVec3::from_array(h.world_pos.0);
            markers.push(Marker { pos: pnt, shape: MarkerShape::Square, size: 3.5, color: lk.handle, on_top: true });
            if labels_on {
                let kind = match h.kind {
                    p::HandleKind::Point => trs!("Punkt", "point"),
                    p::HandleKind::Radius => trs!("Radius", "radius"),
                    p::HandleKind::Height => trs!("Höhe", "height"),
                    p::HandleKind::Angle { .. } => trs!("Winkel", "angle"),
                    p::HandleKind::Plane { .. } => trs!("Ebene", "plane"),
                    p::HandleKind::Scale { .. } => trs!("Skala", "scale"),
                };
                labels.push(Label {
                    pos: pnt,
                    text: tr!("{kind} Z.{}", "{kind} l.{}", h.range.line),
                    color: lk.handle_label,
                    offset: (6.0, 4.0),
                    priority: 0,
                });
            }
        }
    }
    if plan.shows("edges") {
        // hidden edges dashed on top of everything
        for (mesh, _, w, _, _, _) in &meshes {
            for seg in mesh.edge_segments.chunks_exact(6) {
                let a = w.transform_point3(DVec3::new(seg[0] as f64, seg[1] as f64, seg[2] as f64));
                let b = w.transform_point3(DVec3::new(seg[3] as f64, seg[4] as f64, seg[5] as f64));
                lines.push(top_line(vec![a, b], lk.hidden, 1.0, true));
            }
        }
    }
    if labels_on {
        for pt in sc.parts.iter().filter(|pt| plan.parts.is_empty() || plan.parts.contains(&pt.name)) {
            if let Some(b) = &pt.bounds {
                let (lo, hi) = (DVec3::from_array(b.min.0), DVec3::from_array(b.max.0));
                let pos = DVec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, hi.z);
                labels.push(Label {
                    pos,
                    text: pt.name.clone(),
                    color: lk.part_label,
                    offset: (-8.0, -20.0),
                    priority: 5,
                });
            }
        }
    }
    if (plan.shows("bbox") || plan.shows("dims"))
        && let Some(b) = &subject
    {
        let (lo, hi) = (DVec3::from_array(b.min.0), DVec3::from_array(b.max.0));
        let c = |i: u32| {
            DVec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            )
        };
        for (a, b2) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)]
        {
            lines.push(top_line(vec![c(a), c(b2)], lk.bbox, 1.3, false));
        }
        if plan.shows("dims") {
            let s = hi - lo;
            let dims = [
                ((lo + DVec3::new(s.x / 2.0, 0.0, 0.0)), format!("B {} mm", num(s.x)), (-30.0, 6.0)),
                ((DVec3::new(hi.x, lo.y + s.y / 2.0, lo.z)), format!("T {} mm", num(s.y)), (8.0, 4.0)),
                ((DVec3::new(hi.x, lo.y, lo.z + s.z / 2.0)), format!("H {} mm", num(s.z)), (8.0, -6.0)),
            ];
            for (pos, text, off) in dims {
                labels.push(Label { pos, text, color: lk.dims_label, offset: off, priority: 6 });
            }
        }
    }
    lines.append(&mut keepout_lines);
    let n_asserts = if plan.parts.is_empty() {
        assert_overlay(sc, plan, &mut meshes, (&mut lines, &mut markers, &mut labels))
    } else {
        0
    };
    let n_failures = failure_overlay(sc, &m.diagnostics, *lk, (&mut lines, &mut markers, &mut labels));
    if n_failures > 0 {
        notes.push(tr!(
            "{n_failures} Fehlerstelle(n) rot markiert (Operanden berühren sich tangential; Diagnose: at)",
            "{n_failures} error place(s) marked red (operands touch tangentially; diagnostic: at)"
        ));
    }
    let counts = json!({
        "grids": n_grids, "grid_points": n_points, "anchors": n_anchors, "handles": n_handles,
        "labels": labels.len(), "lines": lines.len(), "markers": markers.len(), "asserts": n_asserts,
        "failures": n_failures,
    });
    Ok(Built { meshes, lines, markers, labels, triangles, visible, highlighted, subject, counts, notes })
}

/// Places of failed operations (`NodeMessage::at`: the operands of a Boolean touch
/// tangentially there): the contact edges red on top, a red dot and a label at the place.
/// Always drawn, like the assertion results. Returns the number of places.
fn failure_overlay(
    sc: &p::Scene,
    diags: &[p::Diagnostic],
    lk: Look,
    (lines, markers, labels): (&mut Vec<Polyline>, &mut Vec<Marker>, &mut Vec<Label>),
) -> usize {
    let mut n = 0;
    for node in sc.nodes.iter().filter(|x| x.status != p::NodeStatus::Skipped) {
        let w = mat(&node.display_world);
        for msg in &node.messages {
            let Some(at) = &msg.at else { continue };
            n += 1;
            for e in &msg.edges {
                let pts: Vec<DVec3> = e.iter().map(|q| w.transform_point3(DVec3::from_array(q.0))).collect();
                lines.push(top_line(pts, lk.failed, 3.0, false));
            }
            let pos = w.transform_point3(DVec3::from_array(at.0));
            markers.push(Marker { pos, shape: MarkerShape::Dot, size: 6.0, color: lk.failed, on_top: true });
            // the lines of the touching operands (from the diagnostic), else the operation's
            let ops: Vec<String> = diags
                .iter()
                .find(|d| d.node == Some(node.idx) && d.message == msg.message)
                .and_then(|d| d.nodes.as_ref())
                .map(|ns| {
                    ns.iter().filter_map(|&k| sc.nodes.get(k as usize)).map(|k| k.range.line.to_string()).collect()
                })
                .unwrap_or_default();
            let lines_text = if ops.is_empty() { node.range.line.to_string() } else { ops.join(" + ") };
            labels.push(Label {
                pos,
                text: tr!("Berührung Z.{lines_text}", "contact l.{lines_text}"),
                color: lk.failed,
                offset: (9.0, -16.0),
                priority: 7,
            });
        }
    }
    n
}

/// Mesh of an assertion overlay (context coordinates, flat normals).
fn assert_mesh(a: &p::AssertMesh) -> Mesh {
    let mut normals = vec![0.0f32; a.positions.len()];
    for t in a.indices.chunks_exact(3) {
        let v = |i: u32| {
            let k = i as usize * 3;
            DVec3::new(a.positions[k] as f64, a.positions[k + 1] as f64, a.positions[k + 2] as f64)
        };
        let n = (v(t[1]) - v(t[0])).cross(v(t[2]) - v(t[0])).normalize_or_zero();
        for &i in t {
            let k = i as usize * 3;
            normals[k..k + 3].copy_from_slice(&[n.x as f32, n.y as f32, n.z as f32]);
        }
    }
    Mesh { positions: a.positions.clone(), normals, indices: a.indices.clone(), ..Default::default() }
}

/// Results of `assert_keepout`/`assert_disjoint`/`assert_intersect`/`assert_touches` (Plan
/// 21 C): intersection bodies red (outline on top), contact faces green, gaps and too small
/// clearances as dimension lines with foot points. Labels for violations, with
/// `--show asserts` for every pair. Returns the number of drawn pairs.
fn assert_overlay(
    sc: &p::Scene,
    plan: &Plan,
    meshes: &mut Vec<Owned>,
    (lines, markers, labels): (&mut Vec<Polyline>, &mut Vec<Marker>, &mut Vec<Label>),
) -> usize {
    let lk = plan.look;
    let all = plan.shows("asserts");
    let mut n = 0;
    for a in sc.asserts.iter().filter(|a| a.checked) {
        for pr in &a.pairs {
            let mut drawn = false;
            if let Some(o) = &pr.overlap
                && (!pr.ok || all)
            {
                // a wanted intersection (assert_intersect) green, a violation red
                let (fill, edge) = if pr.ok { (lk.contact, lk.contact) } else { (lk.overlap, lk.failed) };
                meshes.push((assert_mesh(o), fill, DMat4::IDENTITY, None, true, Material::default()));
                // feature edges on top: visible inside the bodies
                for e in o.edges.chunks_exact(6) {
                    let x = DVec3::new(e[0] as f64, e[1] as f64, e[2] as f64);
                    let y = DVec3::new(e[3] as f64, e[4] as f64, e[5] as f64);
                    lines.push(top_line(vec![x, y], edge, 2.0, false));
                }
                drawn = true;
            }
            if let Some(c) = &pr.contact_mesh {
                meshes.push((assert_mesh(c), lk.contact, DMat4::IDENTITY, None, true, Material::default()));
                drawn = true;
            }
            let contact_c = if pr.ok { lk.contact } else { lk.failed };
            for l in &pr.contact_lines {
                let pts: Vec<DVec3> = l.iter().map(|q| DVec3::from_array(q.0)).collect();
                if pts.len() == 1 {
                    markers.push(Marker {
                        pos: pts[0],
                        shape: MarkerShape::Dot,
                        size: 4.5,
                        color: contact_c,
                        on_top: true,
                    });
                } else {
                    lines.push(top_line(pts, contact_c, 2.4, false));
                }
                drawn = true;
            }
            let gap = matches!(pr.problem.as_deref(), Some("gap" | "too_close" | "apart"));
            if pr.points.len() == 2 && (gap || all) && pr.distance.is_some_and(|d| d > 0.0) {
                let (pa, pb) = (DVec3::from_array(pr.points[0].0), DVec3::from_array(pr.points[1].0));
                let col = if pr.ok { lk.dims_label } else { lk.failed };
                lines.push(top_line(vec![pa, pb], col, 2.0, false));
                for q in [pa, pb] {
                    markers.push(Marker { pos: q, shape: MarkerShape::Dot, size: 3.5, color: col, on_top: true });
                }
                drawn = true;
            }
            if !drawn {
                continue;
            }
            n += 1;
            if pr.ok && !all {
                continue;
            }
            let pos = pr
                .focus
                .as_ref()
                .map(|b| (DVec3::from_array(b.min.0) + DVec3::from_array(b.max.0)) * 0.5)
                .or_else(|| pr.points.first().map(|q| DVec3::from_array(q.0)));
            let Some(pos) = pos else { continue };
            let text = match pr.problem.as_deref() {
                Some("overlap") => match pr.volume.filter(|v| *v > 0.0) {
                    Some(v) => tr!("Schnitt {} mm³", "intersection {} mm³", num(v)),
                    None => tr!("Überschneidung", "overlap"),
                },
                Some("too_close") => format!("{} mm < {}", num(pr.distance.unwrap_or(0.0)), num(a.clearance)),
                Some("gap") => tr!("Spalt {} mm", "gap {} mm", num(pr.distance.unwrap_or(0.0))),
                Some("apart") => tr!("kein Schnitt, {} mm", "no intersection, {} mm", num(pr.distance.unwrap_or(0.0))),
                Some("line_contact") => tr!("Linienkontakt", "line contact"),
                Some("point_contact") => tr!("Punktkontakt", "point contact"),
                _ => match (pr.contact_area, pr.volume.filter(|v| *v > 0.0)) {
                    (Some(ar), _) => tr!("Kontakt {} mm²", "contact {} mm²", num(ar)),
                    (None, Some(v)) => tr!("Schnitt {} mm³", "intersection {} mm³", num(v)),
                    _ => match &pr.contact {
                        Some(c) => c.clone(),
                        None => continue,
                    },
                },
            };
            let color = if pr.ok { lk.dims_label } else { lk.failed };
            labels.push(Label { pos, text, color, offset: (8.0, -10.0), priority: 7 });
        }
    }
    n
}

/// With a section, a model made of parts is drawn part by part instead of as the root's single
/// mesh: touching parts (lid on its case) then get their own section caps, hatched in
/// alternating directions and outlined by their own cut contour, instead of one merged cap.
/// `None` unless the parts make up the whole root (below it only parts, and transforms, groups
/// and loops leading to them).
fn section_parts(sc: &p::Scene, plan: &Plan) -> Option<Vec<u32>> {
    plan.section?;
    if sc.parts.len() < 2 {
        return None;
    }
    fn covered(sc: &p::Scene, n: u32, out: &mut Vec<u32>) -> bool {
        let node = &sc.nodes[n as usize];
        if node.dim == Some(p::Dim::Two) || node.flags.disabled || node.flags.background {
            return true;
        }
        if sc.parts.iter().any(|pt| pt.node == n) {
            out.push(n);
            return true;
        }
        let structural = matches!(
            node.label.as_str(),
            "translate" | "rotate" | "mirror" | "multmatrix" | "for" | "group" | "union" | "color" | "scale"
        );
        structural && !node.children.is_empty() && node.children.iter().all(|&c| covered(sc, c, out))
    }
    let root = sc.nodes.get(sc.root? as usize)?;
    let mut out = Vec::new();
    root.children.iter().all(|&c| covered(sc, c, &mut out)).then_some(out).filter(|o| o.len() > 1)
}

/// A grid whose every reference lies in the 2D profile of one `rotate_extrude` (`polygon([g[…],
/// …])` below it, through 2D operations only) describes that profile: x = radius, y = height.
/// The profile is swept from the extrusion's xz plane, so the grid is drawn there instead of in
/// the xy plane of its module: the frame `RE · Rx(90°) · RE⁻¹ · N · frame_module` (N the node
/// of the referencing statement, RE the extrusion; grid values are coordinates in N's system).
/// `None` for every other grid, or when references sit in different extrusions or outside a
/// node (variables).
fn profile_grid_frame(sc: &p::Scene, g: &p::Grid) -> Option<DMat4> {
    let refs: Vec<&p::Ref> = sc
        .refs
        .iter()
        .filter(|r| match r.target {
            p::RefTarget::Grid { grid, .. } | p::RefTarget::Plane { grid, .. } => grid == g.id,
            p::RefTarget::Anchor { .. } => false,
        })
        .collect();
    if refs.is_empty() || g.planes.iter().any(|pl| pl.axis == 2 && !pl.implicit) {
        return None;
    }
    let mut found: Option<(u32, DMat4)> = None;
    for r in refs {
        let stmt = r.stmt?;
        let mut any = false;
        for n in sc.nodes.iter().filter(|n| n.stmt == Some(stmt)) {
            if n.dim != Some(p::Dim::Two) {
                return None;
            }
            // up through 2D nodes to the extrusion
            let mut cur = n.parent;
            let re = loop {
                let q = sc.nodes.get(cur? as usize)?;
                if matches!(q.op_info, Some(p::OpInfo::RotateExtrude { .. })) {
                    break q;
                }
                if q.dim != Some(p::Dim::Two) {
                    return None;
                }
                cur = q.parent;
            };
            let rw = crate::mat(&re.world);
            let f = rw
                * DMat4::from_rotation_x(std::f64::consts::FRAC_PI_2)
                * rw.inverse()
                * crate::mat(&n.world)
                * crate::mat(&g.frame_module);
            match &found {
                Some((idx, f0)) if *idx != re.idx || !f0.abs_diff_eq(f, 1e-9) => return None,
                _ => found = Some((re.idx, f)),
            }
            any = true;
        }
        if !any {
            return None;
        }
    }
    found.map(|(_, f)| f)
}

/// Lines (plane rectangles at the grid's border), points and labels of one grid; `frame`
/// overrides the grid's own frame (see [`profile_grid_frame`]).
fn grid_overlay(
    g: &p::Grid,
    frame: Option<DMat4>,
    subject: Option<&p::Bounds>,
    size: f64,
    labels_on: bool,
    lk: &Look,
    (lines, markers, labels): (&mut Vec<Polyline>, &mut Vec<Marker>, &mut Vec<Label>),
) -> usize {
    let frame = frame.unwrap_or_else(|| mat(&g.frame));
    let inv = frame.inverse();
    // subject bounds in the grid system (for axes with a single plane)
    let local_box = subject.map(|b| {
        let (lo, hi) = (DVec3::from_array(b.min.0), DVec3::from_array(b.max.0));
        let (mut l, mut h) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for i in 0..8 {
            let c = DVec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            );
            let q = inv.transform_point3(c);
            l = l.min(q);
            h = h.max(q);
        }
        (l, h)
    });
    let planes: [Vec<&p::Plane>; 3] =
        std::array::from_fn(|a| g.planes.iter().filter(|pl| pl.axis == a as u8).collect());
    let mut ext = [(0.0f64, 0.0f64); 3];
    for a in 0..3 {
        let vals: Vec<f64> = planes[a].iter().map(|pl| pl.value).collect();
        let (mut lo, mut hi) = vals.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| (l.min(*v), h.max(*v)));
        if hi - lo < 1e-9 {
            match local_box {
                Some((l, h)) => {
                    lo = lo.min(l[a]);
                    hi = hi.max(h[a]);
                }
                None => {
                    lo -= size * 0.25;
                    hi += size * 0.25;
                }
            }
        }
        ext[a] = (lo, hi);
    }
    let margin = (size * 0.04).max(0.5);
    let ext: [(f64, f64); 3] = ext.map(|(l, h)| (l - margin, h + margin));
    let w = |v: DVec3| frame.transform_point3(v);
    for a in 0..3 {
        let (b, c) = ((a + 1) % 3, (a + 2) % 3);
        for pl in &planes[a] {
            if pl.implicit {
                continue;
            }
            let corner = |u: f64, v: f64| {
                let mut q = DVec3::ZERO;
                q[a] = pl.value;
                q[b] = u;
                q[c] = v;
                w(q)
            };
            let (b0, b1, c0, c1) = (ext[b].0, ext[b].1, ext[c].0, ext[c].1);
            let pts = vec![corner(b0, c0), corner(b1, c0), corner(b1, c1), corner(b0, c1), corner(b0, c0)];
            lines.push(top_line(pts, lk.grid, 1.0, pl.literal.is_none()));
            // Indexed planes (`rippe[0]` … `rippe[7]`) get one label for the whole series at
            // its first plane, `g.rippe[0..7]` (the label font has no ellipsis): one label per plane crowded the image (S4).
            let series: Vec<u32> =
                planes[a].iter().filter(|q| q.index.is_some() && q.base == pl.base).filter_map(|q| q.index).collect();
            let text = match pl.index {
                Some(i) if series.len() > 2 => {
                    if i != *series.iter().min().unwrap_or(&i) {
                        continue;
                    }
                    format!("{}.{}[{}..{}]", g.name, pl.base, i, series.iter().max().unwrap_or(&i))
                }
                _ => format!("{}.{}", g.name, pl.name),
            };
            labels.push(Label { pos: corner(b0, c0), text, color: lk.grid_label, offset: (3.0, 2.0), priority: 3 });
        }
    }
    // intersection points
    let mut count = 0;
    let point_labels = labels_on && planes.iter().map(|p| p.len().max(1)).product::<usize>() <= MAX_POINT_LABELS;
    let axis_planes: [Vec<(String, f64)>; 3] = std::array::from_fn(|a| {
        if planes[a].is_empty() {
            vec![("_".to_string(), 0.0)]
        } else {
            planes[a].iter().map(|pl| (pl.name.clone(), pl.value)).collect()
        }
    });
    'outer: for (nx, x) in &axis_planes[0] {
        for (ny, y) in &axis_planes[1] {
            for (nz, z) in &axis_planes[2] {
                if count >= 1000 {
                    break 'outer;
                }
                count += 1;
                let pnt = w(DVec3::new(*x, *y, *z));
                markers.push(Marker {
                    pos: pnt,
                    shape: MarkerShape::Dot,
                    size: 2.5,
                    color: lk.grid_point,
                    on_top: true,
                });
                if point_labels {
                    labels.push(Label {
                        pos: pnt,
                        text: format!("{}[{nx},{ny},{nz}]", g.name),
                        color: lk.grid_point_label,
                        offset: (4.0, 3.0),
                        priority: 1,
                    });
                }
            }
        }
    }
    count
}

/// Settings of `--style studio` (JSON `studio`).
fn studio_json(s: &Studio, res: &Resolved) -> Json {
    let first = s.lights.first();
    let lights: Vec<Json> = s
        .lights
        .iter()
        .map(|l| {
            json!({ "azimuth": l.azimuth, "elevation": l.elevation, "intensity": l.intensity,
                    "color": palette::hex_string([l.color[0], l.color[1], l.color[2], 1.0]) })
        })
        .collect();
    let materials: Vec<Json> = res
        .materials
        .iter()
        .map(|(sel, m)| {
            let r = |x: f32| (x as f64 * 1000.0).round() / 1000.0;
            json!({ "target": sel.text(), "metalness": r(m.metalness), "roughness": r(m.roughness),
                    "clearcoat": r(m.clearcoat) })
        })
        .collect();
    json!({
        "light_azimuth": first.map(|l| l.azimuth), "light_elevation": first.map(|l| l.elevation),
        "lights": lights,
        "light_intensity": s.light_intensity, "ambient": s.ambient,
        "environment": s.environment.name(),
        "shadows": s.shadows, "shadow_softness": s.shadow_softness,
        "ao": s.ao, "ao_radius": s.ao_radius, "ao_strength": s.ao_strength,
        "ground": s.ground, "ground_color": palette::hex_string(s.ground_color),
        "edges": s.edges,
        "materials": materials,
        "render_block": res.block_line.map(|l| json!({ "line": l })),
    })
}

/// What the studio look used in one view (JSON `views[].studio`).
fn studio_view_json(s: &StudioInfo) -> Json {
    let r = |v: f64| num(v).parse::<f64>().unwrap_or(v);
    json!({
        "light_dir": [r(s.light_dir.x), r(s.light_dir.y), r(s.light_dir.z)],
        "ground_z": s.ground_z.map(r),
        "ao_radius_mm": s.ao_radius.map(r),
        "shadow_map": s.shadow_map,
    })
}

fn camera_json(c: &CameraInfo) -> Json {
    let (proj, fov) = match c.projection {
        Projection::Ortho => ("ortho", None),
        Projection::Perspective { fov_deg } => ("perspective", Some(fov_deg)),
    };
    let v = |d: DVec3| {
        json!([
            num(d.x).parse::<f64>().unwrap_or(d.x),
            num(d.y).parse::<f64>().unwrap_or(d.y),
            num(d.z).parse::<f64>().unwrap_or(d.z)
        ])
    };
    json!({
        "projection": proj, "fov_deg": fov,
        "azimuth": num(c.azimuth).parse::<f64>().unwrap_or(c.azimuth),
        "elevation": num(c.elevation).parse::<f64>().unwrap_or(c.elevation),
        "dir": v(c.dir), "up": v(c.up), "right": v(c.right), "target": v(c.target), "eye": v(c.eye),
        "scale_px_per_mm": c.scale, "width_mm": c.width_mm, "height_mm": c.height_mm,
    })
}

/// Renders the evaluated model `m` with `plan` into `output`; (JSON fields, text line).
pub(crate) fn render_model(m: &mut Model, plan: &Plan, output: &Path) -> Result<(Json, String), Fail> {
    let Some(sc) = m.scene.clone() else {
        return Err(Fail::Usage(tr!("keine Szene (Syntaxfehler)", "no scene (syntax error)")));
    };
    let res = plan.resolve(sc.render.as_ref())?;
    let built = build(m, &sc, plan, &res)?;
    let items: Vec<Item> = built
        .meshes
        .iter()
        .map(|(mesh, col, w, edges, cap, mt)| {
            Item::new(mesh, *col).with_transform(*w).with_edges(*edges).with_cap(*cap).with_material(*mt)
        })
        .collect();
    let scene =
        Scene { items, lines: built.lines.clone(), markers: built.markers.clone(), labels: built.labels.clone() };
    let fit = match &plan.fit {
        None => None,
        Some(spec) => {
            let nodes = target_nodes(&sc, spec)?;
            nodes_bounds(&sc, &nodes).map(|b| (DVec3::from_array(b.min.0), DVec3::from_array(b.max.0)))
        }
    };
    let n = plan.views.len();
    let cols = match n {
        1 => 1,
        2 | 4 => 2,
        _ => 3,
    };
    let rows = n.div_ceil(cols);
    let gap = if plan.sheet { 8 } else { 0 };
    let (tw, th) = if plan.sheet {
        let tw = (plan.size.0.saturating_sub(gap * (cols as u32 + 1)) / cols as u32).max(16);
        let th = (plan.size.1.saturating_sub(gap * (rows as u32 + 1)) / rows as u32).max(16);
        (tw, th)
    } else {
        plan.size
    };
    let base = RenderOptions {
        width: tw,
        height: th,
        axes: plan.axes,
        projection: plan.projection,
        target: plan.target,
        fit,
        zoom: plan.zoom,
        section: plan.section,
        background: res.background.0,
        background_bottom: res.background.1,
        studio: res.studio.clone(),
        cap_color: plan.look.cap,
        title_color: plan.look.text,
        label_background: plan.look.label_background,
        ..Default::default()
    };
    // Contact sheet: one target (centre of the fitted geometry) and the smallest fitting scale.
    let mut common = base.clone();
    if plan.sheet {
        let pts_box = fit.or_else(|| {
            let b = built.subject.as_ref()?;
            let all = nodes_bounds(&sc, &sc.root.into_iter().collect::<Vec<_>>()).unwrap_or(b.clone());
            let u = union(Some(all), b)?;
            Some((DVec3::from_array(u.min.0), DVec3::from_array(u.max.0)))
        });
        if common.target.is_none() {
            common.target = pts_box.map(|(lo, hi)| (lo + hi) / 2.0);
        }
        let s = plan
            .views
            .iter()
            .map(|(_, v)| cad_render::fit_scale(&scene, &RenderOptions { view: *v, ..common.clone() }))
            .fold(f64::INFINITY, f64::min);
        if s.is_finite() {
            common.scale = Some(s);
        }
    }
    let mut tiles = Vec::new();
    let mut views_json = Vec::new();
    for (k, (name, view)) in plan.views.iter().enumerate() {
        let title = if plan.sheet {
            Some(match name.as_str() {
                "iso" => "iso".to_string(),
                "front" => tr!("front (von -y)", "front (from -y)"),
                "back" => tr!("back (von +y)", "back (from +y)"),
                "top" => tr!("top (von +z)", "top (from +z)"),
                "bottom" => tr!("bottom (von -z)", "bottom (from -z)"),
                "left" => tr!("left (von -x)", "left (from -x)"),
                "right" => tr!("right (von +x)", "right (from +x)"),
                other => other.to_string(),
            })
        } else {
            None
        };
        let opts = RenderOptions { view: *view, title, ..common.clone() };
        let (img, cam) = render_info(&scene, &opts);
        let (x, y) = if plan.sheet { tile_origin(k, cols, (tw, th), gap) } else { (0, 0) };
        views_json.push(json!({ "name": name, "x": x, "y": y, "width": tw, "height": th, "camera": camera_json(&cam),
            "studio": cam.studio.as_ref().map(studio_view_json) }));
        tiles.push(img);
    }
    let img = if plan.sheet { contact_sheet(&tiles, cols, gap, res.background.0) } else { tiles.remove(0) };
    crate::host::write_file(output, &img.to_png()).map_err(|e| Fail::Usage(format!("{}: {e}", output.display())))?;

    let section = plan.section.map(|s| {
        json!({ "point": s.point.to_array(), "normal": s.normal.to_array(), "keeps": "(q - point) · normal <= 0",
                "method": "renderer clipping + cap" })
    });
    let background_json: Vec<String> = {
        let (t, b) = res.background;
        std::iter::once(t).chain(b).map(palette::hex_string).collect()
    };
    let j = json!({
        "output": output.display().to_string(),
        "width": img.width,
        "height": img.height,
        "view": plan.views[0].0,
        "views": views_json,
        "camera": views_json[0]["camera"].clone(),
        "line": plan.lines.first(),
        "lines": plan.lines,
        "nodes": plan.nodes,
        "parts": plan.parts,
        "highlighted": built.highlighted,
        "visible_nodes": built.visible,
        "triangles": built.triangles,
        "show": plan.show,
        "overlays": built.counts,
        "section": section,
        "xray": plan.xray,
        "theme": if plan.look.theme == Theme::Dark { "dark" } else { "light" },
        "style": if res.studio.is_some() { "studio" } else { "standard" },
        "studio": res.studio.as_ref().map(|s| studio_json(s, &res)),
        "background": background_json,
        "bounds": bounds_json(&built.subject),
        "notes": built.notes,
    });
    let mut t = format!(
        "{}: {}x{}, {} {}",
        output.display(),
        img.width,
        img.height,
        if plan.sheet { trs!("Ansichten", "views") } else { trs!("Ansicht", "view") },
        plan.views.iter().map(|v| v.0.as_str()).collect::<Vec<_>>().join(", ")
    );
    if !plan.lines.is_empty() || !plan.nodes.is_empty() {
        let labels: Vec<String> =
            built.highlighted.iter().map(|&h| format!("[{}] {}", h, sc.nodes[h as usize].label)).collect();
        let what = plan.lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", ");
        if labels.is_empty() {
            t.push_str(&tr!(", Zeile {what}: keine Geometrie", ", line {what}: no geometry"));
        } else if what.is_empty() {
            t.push_str(&tr!(", Knoten: {}", ", nodes: {}", labels.join(", ")));
        } else {
            t.push_str(&tr!(", Zeile {what}: {}", ", line {what}: {}", labels.join(", ")));
        }
    }
    if !plan.parts.is_empty() {
        t.push_str(&tr!(", Bauteil {}", ", part {}", plan.parts.join(", ")));
    }
    if let Some(ctx) = &m.context {
        t.push_str(&tr!(", Kontext Modul {ctx}", ", context module {ctx}"));
    }
    if let Some(s) = plan.section {
        t.push_str(&tr!(
            ", Schnitt durch {} Normale {}",
            ", section through {} normal {}",
            vec3d(s.point),
            vec3d(s.normal)
        ));
    }
    if plan.xray {
        t.push_str(trs!(", Röntgen", ", x-ray"));
    }
    if plan.look.theme == Theme::Dark {
        t.push_str(trs!(", dunkel", ", dark"));
    }
    if res.studio.is_some() {
        t.push_str(trs!(", Studio", ", studio"));
    }
    if let Some(b) = &built.subject {
        let s: Vec<String> = (0..3).map(|i| num(b.max.0[i] - b.min.0[i])).collect();
        t.push_str(&tr!(", Hülle {} mm", ", bounds {} mm", s.join(" × ")));
    }
    t.push('\n');
    for n in &built.notes {
        t.push_str(&tr!("Hinweis: {n}\n", "note: {n}\n"));
    }
    Ok((j, t))
}

pub(crate) fn vec3d(v: DVec3) -> String {
    format!("[{}, {}, {}]", num(v.x), num(v.y), num(v.z))
}

pub(crate) fn command(c: &Common, output: &Path, a: &ViewArgs) -> Result<Outcome, Fail> {
    let plan = Plan::from_args(a)?;
    let t0 = std::time::Instant::now();
    let mut m = load(c, plan.lines.first().copied(), a.whole_file, true)?;
    let load_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let mut j = m.base_json("render");
    j["line"] = json!(plan.lines.first());
    if m.scene.is_none() {
        let t = m.diagnostics_text();
        return Ok(Outcome { json: j, text: t, errors: true, exit: None });
    }
    let t1 = std::time::Instant::now();
    let (fields, line) = render_model(&mut m, &plan, output)?;
    if let (Json::Object(dst), Json::Object(src)) = (&mut j, fields) {
        dst.extend(src);
    }
    let stats = m.scene.as_ref().map(|s| s.stats).unwrap_or_default();
    j["timings"] = json!({
        "evaluate_ms": load_ms, "eval_ms": stats.eval_ms, "kernel_ms": stats.kernel_ms,
        "render_ms": t1.elapsed().as_secs_f64() * 1000.0,
    });
    let mut t = m.diagnostics_text();
    t.push_str(&line);
    Ok(Outcome { json: j, text: t, errors: m.has_errors(), exit: None })
}
