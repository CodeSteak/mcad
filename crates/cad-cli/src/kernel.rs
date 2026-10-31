//! Kernel selection: `--kernel smlib|solidcore` (global option) or `MCAD_KERNEL`, default
//! SMLib. The commands work on [`AnyKernel`], which forwards every call to the selected kernel,
//! so `host::new_session` and the command code stay the same for both.
//!
//! solidcore is linked only with the cargo feature `solidcore`; without it [`resolve`] refuses
//! `solidcore` (usage error, exit 2). The daemon keeps one SMLib engine: with solidcore the
//! command runs without the daemon (`client.rs`), sessions inside the daemon always use SMLib.

use std::sync::atomic::{AtomicBool, Ordering};

use cad_i18n::tr;
use cad_kernel::{
    BoolOp, Class, EdgeId, EdgeInfo, ExportFormat, FaceId, FaceInfo, KResult, Kernel, KernelError, Loop, MassProps,
    Mesh, PathInterp, PlacedLoop, PlacedPath, Tag, brep,
};
use glam::{DMat4, DVec2, DVec3};
use kernel_smlib::SmlibKernel;
use kernel_solidcore::SolidcoreKernel;

/// The kernel of the commands of this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Choice {
    #[default]
    Smlib,
    Solidcore,
}

impl Choice {
    pub fn parse(s: &str) -> Option<Choice> {
        match s.trim().to_ascii_lowercase().as_str() {
            "smlib" => Some(Choice::Smlib),
            "solidcore" => Some(Choice::Solidcore),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Choice::Smlib => "smlib",
            Choice::Solidcore => "solidcore",
        }
    }

    /// Kernel id (part of every cache key; `kernel_id` in the JSON output).
    pub fn id(self) -> &'static str {
        match self {
            Choice::Smlib => SmlibKernel::new().id(),
            Choice::Solidcore => kernel_solidcore::KERNEL_ID,
        }
    }

    pub fn new_kernel(self) -> AnyKernel {
        match self {
            Choice::Smlib => AnyKernel::Smlib(SmlibKernel::new()),
            Choice::Solidcore => AnyKernel::Solidcore(Box::new(SolidcoreKernel::new())),
        }
    }
}

static SOLIDCORE: AtomicBool = AtomicBool::new(false);

/// Sets the kernel of this process (main, before the command runs).
pub fn select(c: Choice) {
    SOLIDCORE.store(c == Choice::Solidcore, Ordering::Relaxed);
}

static FAILED: AtomicBool = AtomicBool::new(false);

/// Kernel resolution failed (main, before the command): the output names no kernel.
pub fn select_failed() {
    FAILED.store(true, Ordering::Relaxed);
}

/// `false` after [`select_failed`].
pub fn known() -> bool {
    !FAILED.load(Ordering::Relaxed)
}

pub fn selected() -> Choice {
    if SOLIDCORE.load(Ordering::Relaxed) { Choice::Solidcore } else { Choice::Smlib }
}

/// The kernel from `--kernel`, else `MCAD_KERNEL` (empty: unset), else SMLib. Error (usage,
/// exit 2): unknown name in `MCAD_KERNEL` (clap checks `--kernel`), or solidcore in a build
/// without the feature `solidcore`.
pub fn resolve(flag: Option<&str>) -> Result<Choice, String> {
    let env = std::env::var("MCAD_KERNEL").ok().filter(|v| !v.trim().is_empty());
    let (choice, source) = match (flag, env) {
        (Some(f), _) => (Choice::parse(f), "--kernel"),
        (None, Some(v)) => match Choice::parse(&v) {
            Some(c) => (Some(c), "MCAD_KERNEL"),
            None => {
                return Err(tr!(
                    "MCAD_KERNEL: unbekannter Kernel `{v}` (smlib oder solidcore)",
                    "MCAD_KERNEL: unknown kernel `{v}` (smlib or solidcore)"
                ));
            }
        },
        (None, None) => (Some(Choice::Smlib), ""),
    };
    let choice = choice.unwrap_or_default();
    if choice == Choice::Solidcore && !SolidcoreKernel::available() {
        let what = if source == "--kernel" { "--kernel solidcore" } else { "MCAD_KERNEL=solidcore" };
        return Err(tr!(
            "{what} braucht solidcore: mcad mit `--features solidcore` bauen \
             (cargo build -p cad-cli --features solidcore)",
            "{what} needs solidcore: build mcad with `--features solidcore` \
             (cargo build -p cad-cli --features solidcore)"
        ));
    }
    Ok(choice)
}

/// SMLib or solidcore, chosen at run time.
#[derive(Debug)]
pub enum AnyKernel {
    Smlib(SmlibKernel),
    /// Boxed: the solidcore kernel state is a few hundred bytes, SMLib's one flag.
    Solidcore(Box<SolidcoreKernel>),
}

/// A shape of the kernel that made it.
#[derive(Debug, Clone)]
pub enum AnyShape {
    Smlib(kernel_smlib::Shape),
    Solidcore(kernel_solidcore::Shape),
}

/// A shape passed to the other kernel (cannot happen: one session has one kernel).
fn mixed() -> KernelError {
    KernelError::InvalidInput("shape of another kernel".into())
}

/// `$body` on the inner kernel `$k` (constructors; the result is wrapped).
macro_rules! make {
    ($self:ident, |$k:ident| $body:expr) => {
        match $self {
            AnyKernel::Smlib($k) => $body.map(AnyShape::Smlib),
            AnyKernel::Solidcore($k) => $body.map(AnyShape::Solidcore),
        }
    };
}

/// `$body` on the inner kernel `$k` and the inner shape `$x` of `$s`; `=> shape` wraps the
/// result.
macro_rules! on {
    ($self:ident, $s:expr, |$k:ident, $x:ident| $body:expr => shape) => {
        match ($self, $s) {
            (AnyKernel::Smlib($k), AnyShape::Smlib($x)) => $body.map(AnyShape::Smlib),
            (AnyKernel::Solidcore($k), AnyShape::Solidcore($x)) => $body.map(AnyShape::Solidcore),
            _ => Err(mixed()),
        }
    };
    ($self:ident, $s:expr, |$k:ident, $x:ident| $body:expr) => {
        match ($self, $s) {
            (AnyKernel::Smlib($k), AnyShape::Smlib($x)) => $body,
            (AnyKernel::Solidcore($k), AnyShape::Solidcore($x)) => $body,
            _ => Err(mixed()),
        }
    };
}

fn smlib_shapes(shapes: &[AnyShape]) -> KResult<Vec<kernel_smlib::Shape>> {
    shapes
        .iter()
        .map(|s| match s {
            AnyShape::Smlib(s) => Ok(s.clone()),
            AnyShape::Solidcore(_) => Err(mixed()),
        })
        .collect()
}

fn solidcore_shapes(shapes: &[AnyShape]) -> KResult<Vec<kernel_solidcore::Shape>> {
    shapes
        .iter()
        .map(|s| match s {
            AnyShape::Solidcore(s) => Ok(s.clone()),
            AnyShape::Smlib(_) => Err(mixed()),
        })
        .collect()
}

impl Kernel for AnyKernel {
    type Shape = AnyShape;

    fn id(&self) -> &'static str {
        match self {
            AnyKernel::Smlib(k) => k.id(),
            AnyKernel::Solidcore(k) => k.id(),
        }
    }

    fn make_box(&mut self, min: DVec3, max: DVec3, tag: Tag) -> KResult<AnyShape> {
        make!(self, |k| k.make_box(min, max, tag))
    }

    fn make_sphere(&mut self, r: f64, tag: Tag) -> KResult<AnyShape> {
        make!(self, |k| k.make_sphere(r, tag))
    }

    fn make_cylinder(&mut self, h: f64, r1: f64, r2: f64, tag: Tag) -> KResult<AnyShape> {
        make!(self, |k| k.make_cylinder(h, r1, r2, tag))
    }

    fn make_torus(&mut self, big_r: f64, r: f64, tag: Tag) -> KResult<AnyShape> {
        make!(self, |k| k.make_torus(big_r, r, tag))
    }

    fn make_polyhedron(&mut self, points: &[DVec3], faces: &[Vec<u32>], tag: Tag) -> KResult<AnyShape> {
        make!(self, |k| k.make_polyhedron(points, faces, tag))
    }

    fn transform(&mut self, s: &AnyShape, m: &DMat4) -> KResult<AnyShape> {
        on!(self, s, |k, s| k.transform(s, m) => shape)
    }

    fn boolean(&mut self, op: BoolOp, shapes: &[AnyShape], tag: Tag) -> KResult<AnyShape> {
        match self {
            AnyKernel::Smlib(k) => k.boolean(op, &smlib_shapes(shapes)?, tag).map(AnyShape::Smlib),
            AnyKernel::Solidcore(k) => k.boolean(op, &solidcore_shapes(shapes)?, tag).map(AnyShape::Solidcore),
        }
    }

    fn fillet(&mut self, s: &AnyShape, edges: &[EdgeId], r: f64, tag: Tag) -> KResult<AnyShape> {
        on!(self, s, |k, s| k.fillet(s, edges, r, tag) => shape)
    }

    fn chamfer(&mut self, s: &AnyShape, edges: &[EdgeId], d: f64, tag: Tag) -> KResult<AnyShape> {
        on!(self, s, |k, s| k.chamfer(s, edges, d, tag) => shape)
    }

    fn law_stations(&mut self, s: &AnyShape, edges: &[EdgeId], counts: &[u32]) -> KResult<Vec<Vec<DVec3>>> {
        on!(self, s, |k, s| k.law_stations(s, edges, counts))
    }

    fn fillet_variable(&mut self, s: &AnyShape, edges: &[EdgeId], radii: &[Vec<f64>], tag: Tag) -> KResult<AnyShape> {
        on!(self, s, |k, s| k.fillet_variable(s, edges, radii, tag) => shape)
    }

    fn extrude(&mut self, p: &PlacedLoop, h: f64, twist: f64, scale: DVec2, tag: Tag) -> KResult<AnyShape> {
        make!(self, |k| k.extrude(p, h, twist, scale, tag))
    }

    fn revolve(&mut self, p: &PlacedLoop, angle: f64, tag: Tag) -> KResult<AnyShape> {
        make!(self, |k| k.revolve(p, angle, tag))
    }

    fn loft(&mut self, sections: &[PlacedLoop], ruled: bool, tag: Tag) -> KResult<AnyShape> {
        make!(self, |k| k.loft(sections, ruled, tag))
    }

    fn sweep(&mut self, p: &Loop, path: &[DVec3], interp: PathInterp, tag: Tag) -> KResult<AnyShape> {
        make!(self, |k| k.sweep(p, path, interp, tag))
    }

    fn sweep_path(&mut self, p: &Loop, path: &PlacedPath, tag: Tag) -> KResult<AnyShape> {
        make!(self, |k| k.sweep_path(p, path, tag))
    }

    fn nurbs_surface(&mut self, rows: &[Vec<DVec3>], tag: Tag) -> KResult<AnyShape> {
        make!(self, |k| k.nurbs_surface(rows, tag))
    }

    fn thicken(&mut self, s: &AnyShape, t: f64, tag: Tag) -> KResult<AnyShape> {
        on!(self, s, |k, s| k.thicken(s, t, tag) => shape)
    }

    fn shell(&mut self, s: &AnyShape, t: f64, open: &[FaceId], tag: Tag) -> KResult<AnyShape> {
        on!(self, s, |k, s| k.shell(s, t, open, tag) => shape)
    }

    fn offset(&mut self, s: &AnyShape, d: f64, tag: Tag) -> KResult<AnyShape> {
        on!(self, s, |k, s| k.offset(s, d, tag) => shape)
    }

    fn edges(&mut self, s: &AnyShape, samples: u32) -> KResult<Vec<EdgeInfo>> {
        on!(self, s, |k, s| k.edges(s, samples))
    }

    fn faces(&mut self, s: &AnyShape, samples: u32) -> KResult<Vec<FaceInfo>> {
        on!(self, s, |k, s| k.faces(s, samples))
    }

    fn classify(&mut self, s: &AnyShape, pts: &[DVec3]) -> KResult<Vec<Class>> {
        on!(self, s, |k, s| k.classify(s, pts))
    }

    fn tessellate(&mut self, s: &AnyShape, tol: f64) -> KResult<Mesh> {
        on!(self, s, |k, s| k.tessellate(s, tol))
    }

    fn mass_props(&mut self, s: &AnyShape) -> KResult<MassProps> {
        on!(self, s, |k, s| k.mass_props(s))
    }

    fn bounds(&mut self, s: &AnyShape) -> KResult<Option<(DVec3, DVec3)>> {
        on!(self, s, |k, s| k.bounds(s))
    }

    fn is_empty(&mut self, s: &AnyShape) -> bool {
        on!(self, s, |k, s| Ok(k.is_empty(s))).unwrap_or(true)
    }

    fn is_solid(&mut self, s: &AnyShape) -> bool {
        on!(self, s, |k, s| Ok(k.is_solid(s))).unwrap_or(false)
    }

    fn brep(&mut self, s: &AnyShape) -> KResult<brep::Brep> {
        on!(self, s, |k, s| k.brep(s))
    }

    fn export(&mut self, shapes: &[AnyShape], fmt: ExportFormat) -> KResult<Vec<u8>> {
        match self {
            AnyKernel::Smlib(k) => k.export(&smlib_shapes(shapes)?, fmt),
            AnyKernel::Solidcore(k) => k.export(&solidcore_shapes(shapes)?, fmt),
        }
    }

    fn shape_bytes(&mut self, s: &AnyShape) -> usize {
        on!(self, s, |k, s| Ok(k.shape_bytes(s))).unwrap_or(16 << 10)
    }

    fn set_draft(&mut self, draft: bool) {
        match self {
            AnyKernel::Smlib(k) => k.set_draft(draft),
            AnyKernel::Solidcore(k) => k.set_draft(draft),
        }
    }
}
