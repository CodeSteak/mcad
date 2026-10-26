//! Public input and output types of an evaluation. Everything is serde-serialisable.

use cad_ir::{NodeIdx, Tree};
use cad_syntax::ast::StmtId;
use cad_syntax::{Diagnostic, Span};
use glam::{DMat4, DVec3};
use serde::{Deserialize, Serialize};

use crate::value::{GridId, InstId, Origin, PValue, Sel};

/// What the evaluation shows (Spec: UI-Modell, Kontext).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Context {
    /// The whole file. `overrides` replace the values of assignments at the top level of the
    /// main file (CLI `-D` without a module, like `openscad -D`): the last assignment of a name
    /// takes the value, expressions that use it compute with it; `$` variables are set even
    /// without an assignment. Other unknown names give a warning.
    File { overrides: Vec<(String, PValue)> },
    /// One module, instantiated with its defaults and the given overrides (parameter
    /// bar, CLI `-D`). The module is the tree root; its grids are `in_context`.
    /// `name` is a module name, or a `::`-separated path for nested modules
    /// (`outer::inner`); a plain name finds the first module of that name.
    Module { name: String, overrides: Vec<(String, PValue)> },
}

impl Default for Context {
    fn default() -> Self {
        Context::File { overrides: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalOptions {
    pub context: Context,
    /// Evaluation steps (expressions, statements, loop iterations) before aborting.
    pub max_steps: u64,
    pub max_module_depth: u32,
    pub max_function_depth: u32,
    /// Native stack the evaluation may use before reporting "recursion too deep"
    /// instead of overflowing. Must be below the stack of the calling thread.
    pub max_stack_bytes: usize,
    /// Maximum number of elements of a single list or range iteration.
    pub max_list_len: usize,
    /// `$preview`.
    pub preview: bool,
    /// `$t`.
    pub time: f64,
    /// Seed for `rands()` without an explicit seed.
    pub seed: u64,
}

impl Default for EvalOptions {
    fn default() -> Self {
        EvalOptions {
            context: Context::default(),
            max_steps: 20_000_000,
            max_module_depth: 1000,
            max_function_depth: 2000,
            max_stack_bytes: 1 << 20,
            max_list_len: 10_000_000,
            preview: true,
            time: 0.0,
            seed: 0x5eed,
        }
    }
}

/// Inline hint at the end of a statement's line (assignments).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hint {
    pub stmt: StmtId,
    /// Span of the statement.
    pub span: Span,
    /// Display text; several different values (loops, several instances) are joined
    /// with ` | `.
    pub text: String,
    pub value: PValue,
    /// Literal that defines the value (number slider / bool switch), if any.
    pub literal: Option<Span>,
    /// Value comes from the context module (not from some other instance).
    pub in_context: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaneInfo {
    /// 0 = x, 1 = y, 2 = z.
    pub axis: u8,
    /// Display and reference name, e.g. `rechts`, `loch[2]`, `_`.
    pub name: String,
    /// Name without index.
    pub base: String,
    pub index: Option<u32>,
    /// Coordinate in the grid system.
    pub value: f64,
    pub origin: Origin,
    /// Span of the plane's value expression (element expression for literal lists).
    pub value_span: Option<Span>,
    /// Editable literal defining the value (also through a variable), if any.
    pub literal: Option<Span>,
    /// Implicit plane `_` at 0 of an axis without planes.
    pub implicit: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridInfo {
    pub id: GridId,
    pub name: String,
    pub stmt: StmtId,
    pub span: Span,
    pub name_span: Span,
    /// Module instance that declared the grid.
    pub module_inst: InstId,
    /// Declared in the context module (shown, editable).
    pub in_context: bool,
    /// Grid system → context root.
    pub frame: DMat4,
    /// Grid system → its module's system.
    pub frame_module: DMat4,
    /// Planes sorted by axis, then value. Grid selectors index into this list.
    pub planes: Vec<PlaneInfo>,
    /// Source range where the grid can be referenced (after the declaration up to the
    /// end of the module body).
    pub visible: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorInfo {
    pub name: String,
    pub stmt: StmtId,
    pub span: Span,
    /// In instance coordinates (the module's system).
    pub point: DVec3,
    pub dir: Option<DVec3>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub id: InstId,
    /// Module name (empty for the file).
    pub module: String,
    /// `k` in `k: modul();`.
    pub label: Option<String>,
    /// The call statement (`None` for the context root).
    pub call_stmt: Option<StmtId>,
    pub call_span: Option<Span>,
    pub def_stmt: Option<StmtId>,
    pub def_span: Option<Span>,
    /// `ModuleCall` node (the tree root for the context root).
    pub node: Option<NodeIdx>,
    pub parent: Option<InstId>,
    /// Instance system → parent module's system (transformation at the call site).
    pub matrix: DMat4,
    /// Instance system → context root.
    pub world: DMat4,
    pub anchors: Vec<AnchorInfo>,
    /// For named instances: source range where `k` can be referenced.
    pub visible: Option<Span>,
    /// Nesting depth (0 = context root).
    pub depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum HandleKind {
    /// Point (position), e.g. cube corner, translate target, polygon point.
    Point,
    /// Radius along `axis` from `base`.
    Radius,
    /// Height along `axis` from `base`.
    Height,
    /// Rotation angle about `axis` (0..2 = x, y, z of `frame`, 3 = custom `axis`).
    Angle { axis: u8 },
    /// Grid plane coordinate along an axis of the grid system.
    Plane { axis: u8 },
    /// Scale factor (`None` = uniform).
    Scale { axis: Option<u8> },
}

impl HandleKind {
    pub fn tag(&self) -> u64 {
        match *self {
            HandleKind::Point => 1,
            HandleKind::Radius => 2,
            HandleKind::Height => 3,
            HandleKind::Angle { axis } => 10 + axis as u64,
            HandleKind::Plane { axis } => 20 + axis as u64,
            HandleKind::Scale { axis } => 30 + axis.map_or(3, |a| a as u64),
        }
    }

    pub fn is_angle(&self) -> bool {
        matches!(self, HandleKind::Angle { .. })
    }
}

/// Which literal encodes a component: `literal = value * scale`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Comp {
    pub span: Span,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Handle {
    /// Stable id (statement, kind, index, instance path); below 2^53.
    pub id: u64,
    pub stmt: StmtId,
    /// Span of the statement.
    pub span: Span,
    pub kind: HandleKind,
    /// Index among handles of the same kind of the statement (box corner 0/1, polygon
    /// point i, …).
    pub index: u32,
    /// Handle's local system → context root.
    pub frame: DMat4,
    /// Position in the local system.
    pub pos: DVec3,
    /// For 1D handles: direction in the local system (`pos = base + axis * value[0]`
    /// for radius, height and plane handles; rotation axis for angles).
    pub axis: Option<DVec3>,
    pub base: Option<DVec3>,
    /// Current values (3 for points, 2 for 2D points, 1 otherwise).
    pub value: Vec<f64>,
    /// Literal per component, `None` = locked.
    pub comps: Vec<Option<Comp>>,
    /// Some component comes from a grid or anchor reference.
    pub has_ref: bool,
    /// Whole argument expression at the use site.
    pub expr_span: Span,
    /// Definition of the value if it came through a variable or parameter.
    pub def_span: Option<Span>,
    /// Literals are reached through a variable (editing moves all uses).
    pub via_var: bool,
    /// Literals are module parameter defaults (editing changes every call).
    pub via_default: bool,
    /// Computed value without literals or references (Spec: Griffe, Regel 3): shown, not
    /// editable. Such handles are listed in `EvalOutput::locked_handles`.
    #[serde(default)]
    pub locked: bool,
    /// Grids visible at the statement (snap targets).
    pub grids: Vec<GridId>,
    /// Named instances visible at the statement (anchor snap targets).
    pub instances: Vec<InstId>,
    pub node: Option<NodeIdx>,
    pub module_inst: InstId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Echo {
    pub stmt: Option<StmtId>,
    pub span: Span,
    /// Text after `ECHO: ` as OpenSCAD prints it.
    pub text: String,
    pub in_context: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamType {
    Number,
    Bool,
    Vector,
    String,
    List,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamSig {
    pub name: String,
    pub span: Span,
    pub default_text: Option<String>,
    pub default_span: Option<Span>,
    /// Default value if it is a constant literal.
    pub default_value: Option<PValue>,
    pub ty: ParamType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleSig {
    pub name: String,
    pub stmt: StmtId,
    pub span: Span,
    pub name_span: Span,
    pub body_span: Span,
    /// Enclosing module names (lexical nesting), outermost first.
    pub path: Vec<String>,
    pub params: Vec<ParamSig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamValue {
    pub name: String,
    pub value: PValue,
    pub overridden: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ContextInfo {
    /// `None` for the file; nested modules with their path (`parts.screw`).
    pub module: Option<String>,
    pub def_stmt: Option<StmtId>,
    pub def_span: Option<Span>,
    /// Lexical path for the path bar: enclosing modules and the module itself.
    pub path: Vec<String>,
    /// Parameter values used for the context module.
    pub params: Vec<ParamValue>,
    /// A `!` modifier made this node the root.
    pub root_modifier: Option<NodeIdx>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RefTarget {
    /// Grid point / line / plane.
    Grid {
        grid: GridId,
        sel: [Sel; 3],
    },
    /// `g.name`.
    Plane {
        grid: GridId,
        plane: u32,
    },
    Anchor {
        inst: InstId,
        anchor: u32,
    },
}

/// A grid or anchor reference expression in the context module (hover highlight,
/// jump to the declaration).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefInfo {
    pub span: Span,
    pub stmt: Option<StmtId>,
    pub target: RefTarget,
}

/// A part (Plan 12.9): a named instance at the top level of the main file (`deckel: deckel();`,
/// also `k[i]: …` in a loop). Parts give the export names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartInfo {
    /// `deckel`, `k[2]`.
    pub name: String,
    pub inst: InstId,
    /// `ModuleCall` node of the instance.
    pub node: NodeIdx,
    /// The named statement.
    pub stmt: StmtId,
    pub span: Span,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvalOutput {
    pub tree: Tree,
    pub diagnostics: Vec<Diagnostic>,
    pub hints: Vec<Hint>,
    /// Parts in declaration order (file context only).
    #[serde(default)]
    pub parts: Vec<PartInfo>,
    pub grids: Vec<GridInfo>,
    /// Module instances; index = id, 0 = context root.
    pub instances: Vec<InstanceInfo>,
    /// Editable handles (a literal or a grid/anchor reference behind some component).
    pub handles: Vec<Handle>,
    /// Display-only handles of computed values (`locked = true`).
    #[serde(default)]
    pub locked_handles: Vec<Handle>,
    pub echoes: Vec<Echo>,
    pub modules: Vec<ModuleSig>,
    pub context: ContextInfo,
    pub refs: Vec<RefInfo>,
    /// Evaluation was aborted (step budget, recursion, failed assertion).
    pub aborted: bool,
    pub steps: u64,
    /// Statements of the segments of every `sketch { … }` node (display, Plan 17.1).
    #[serde(default)]
    pub sketches: Vec<SketchInfo>,
}

/// Source of a `Profile::Sketch` node: per contour and segment (same order as in the profile)
/// the statement that made it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchInfo {
    pub node: NodeIdx,
    /// The `sketch` statement.
    pub stmt: StmtId,
    pub span: Span,
    pub closed: bool,
    pub contours: Vec<SketchContourInfo>,
    /// The segment statements in evaluation order with the drawing state after each (the
    /// sketch editor's current point, Plan 17.12); geometry before `corner()`.
    #[serde(default)]
    pub steps: Vec<SketchStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SketchStepKind {
    Start,
    Seg,
    Close,
    Corner,
}

/// One evaluated segment statement of a sketch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchStep {
    pub stmt: StmtId,
    pub span: Span,
    pub kind: SketchStepKind,
    /// Contour (counted as `start` statements are met, from 0).
    pub contour: u32,
    pub hole: bool,
    /// The segment (`Seg`; `Close`: the closing line, if one was needed), before corners.
    pub seg: Option<cad_ir::Seg>,
    /// Current point after the statement.
    pub point: glam::DVec2,
    /// Direction at the current point (end tangent of the last segment).
    pub tangent: Option<glam::DVec2>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchContourInfo {
    pub hole: bool,
    pub closed: bool,
    /// The `start(…)` statement (`None` without one).
    pub start_stmt: Option<StmtId>,
    pub start_span: Option<Span>,
    pub segs: Vec<SketchSegInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchSegInfo {
    pub stmt: StmtId,
    pub span: Span,
    /// Added by the evaluation (closing line of a contour without `close()`).
    pub implicit: bool,
    /// Error reported for this segment.
    pub error: bool,
}

impl EvalOutput {
    pub fn handle(&self, id: u64) -> Option<&Handle> {
        self.handles.iter().find(|h| h.id == id)
    }

    pub fn part(&self, name: &str) -> Option<&PartInfo> {
        self.parts.iter().find(|p| p.name == name)
    }

    pub fn grid_by_name(&self, name: &str) -> Option<&GridInfo> {
        self.grids.iter().find(|g| g.name == name)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| d.severity == cad_syntax::Severity::Error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEdit {
    pub span: Span,
    pub text: String,
}

/// Applies non-overlapping edits to a source text.
pub fn apply_edits(src: &str, edits: &[TextEdit]) -> String {
    let mut e: Vec<&TextEdit> = edits.iter().collect();
    e.sort_by_key(|e| std::cmp::Reverse(e.span.start));
    let mut s = src.to_string();
    for ed in e {
        s.replace_range(ed.span.start as usize..ed.span.end as usize, &ed.text);
    }
    s
}
