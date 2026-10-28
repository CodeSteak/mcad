//! Conversion of evaluation and engine results into protocol types (byte spans → UTF-16).

use cad_engine::{Evaluation, Status};
use cad_eval::{EvalOutput, PValue};
use cad_ir::{NO_STMT, NodeIdx, Op, Tree};
use cad_kernel::BoolOp;
use cad_syntax::{LineIndex, Span};
use glam::{DMat4, DVec3};

use crate::protocol as p;

/// One file of a project for position conversion.
pub struct FileSrc {
    pub path: String,
    pub text: String,
    pub lines: LineIndex,
    pub role: p::FileRole,
}

impl FileSrc {
    fn new(path: String, text: String, role: p::FileRole) -> Self {
        let lines = LineIndex::new(&text);
        FileSrc { path, text, lines, role }
    }

    fn clamp(&self, b: u32) -> u32 {
        let mut b = b.min(self.text.len() as u32);
        while !self.text.is_char_boundary(b as usize) {
            b -= 1;
        }
        b
    }

    /// 1-based (line, column in UTF-16 units) of a local byte offset.
    fn pos(&self, byte: u32) -> (u32, u32) {
        let byte = self.clamp(byte);
        let line = self.lines.line_of(byte);
        let start = self.lines.line_start(line);
        let col = self.lines.byte_to_utf16(byte) - self.lines.byte_to_utf16(start);
        (line + 1, col + 1)
    }

    fn range(&self, s: Span, file: Option<u32>) -> p::SrcRange {
        let (a, b) = (self.clamp(s.start), self.clamp(s.end.max(s.start)));
        let (line, col) = self.pos(a);
        let (end_line, end_col) = self.pos(b);
        p::SrcRange {
            from: self.lines.byte_to_utf16(a),
            to: self.lines.byte_to_utf16(b),
            line,
            col,
            end_line,
            end_col,
            file,
        }
    }
}

/// Source text(s) with line indices: converts (global) byte spans to [`p::SrcRange`]. `text`
/// and `lines` are the main file's; the other files of a project are in `files` (index =
/// file id, see `cad_syntax::FILE_SHIFT`).
pub struct Source {
    pub text: String,
    pub lines: LineIndex,
    /// All files (index = file id; file 0 = main). Empty for a plain single source.
    pub files: Vec<FileSrc>,
}

impl Source {
    pub fn new(text: String) -> Self {
        let lines = LineIndex::new(&text);
        Source { text, lines, files: Vec::new() }
    }

    /// All files of a loaded project (edits and diagnostics of the main file name no file).
    pub fn from_project(proj: &cad_eval::Project) -> Self {
        let role = |r: cad_eval::FileRole| match r {
            cad_eval::FileRole::Main => p::FileRole::Main,
            cad_eval::FileRole::Include => p::FileRole::Include,
            cad_eval::FileRole::Use => p::FileRole::Use,
        };
        let files: Vec<FileSrc> =
            proj.files.iter().map(|f| FileSrc::new(f.path.clone(), f.text.clone(), role(f.role))).collect();
        Source { text: proj.main_text().to_string(), lines: files[0].lines.clone(), files }
    }

    fn clamp(&self, b: u32) -> u32 {
        let mut b = b.min(self.text.len() as u32);
        while !self.text.is_char_boundary(b as usize) {
            b -= 1;
        }
        b
    }

    /// 1-based (line, column in UTF-16 units) of a byte offset in the main file.
    pub fn pos(&self, byte: u32) -> (u32, u32) {
        let byte = self.clamp(byte);
        let line = self.lines.line_of(byte);
        let start = self.lines.line_start(line);
        let col = self.lines.byte_to_utf16(byte) - self.lines.byte_to_utf16(start);
        (line + 1, col + 1)
    }

    /// File id and its source of a global span (`None` → main file of a plain source).
    fn file_src(&self, s: Span) -> Option<(u32, &FileSrc)> {
        let id = s.file();
        if id == 0 {
            return None;
        }
        self.files.get(id as usize).map(|f| (id, f))
    }

    pub fn range(&self, s: Span) -> p::SrcRange {
        if let Some((id, f)) = self.file_src(s) {
            return f.range(s.local(), Some(id));
        }
        let (a, b) = (self.clamp(s.start), self.clamp(s.end.max(s.start)));
        let (line, col) = self.pos(a);
        let (end_line, end_col) = self.pos(b);
        p::SrcRange {
            from: self.lines.byte_to_utf16(a),
            to: self.lines.byte_to_utf16(b),
            line,
            col,
            end_line,
            end_col,
            file: None,
        }
    }

    /// Path of a span's file (`None` for the main file).
    pub fn path(&self, s: Span) -> Option<String> {
        self.file_src(s).map(|(_, f)| f.path.clone())
    }

    /// Path of a file id (`None` for the main file or an unknown id).
    pub fn path_of_id(&self, id: u32) -> Option<&str> {
        if id == 0 { None } else { self.files.get(id as usize).map(|f| f.path.as_str()) }
    }

    /// Byte offset of a 1-based line start (main file).
    pub fn line_start(&self, line: u32) -> u32 {
        self.lines.line_start(line.saturating_sub(1))
    }

    pub fn edit(&self, e: &cad_eval::TextEdit) -> p::Edit {
        let r = self.range(e.span);
        p::Edit { from: r.from, to: r.to, text: e.text.clone(), file: self.path(e.span) }
    }

    /// The project's files for `Scene.files` (just the main file for a plain source).
    pub fn file_infos(&self, main_path: &str) -> Vec<p::FileInfo> {
        if self.files.is_empty() {
            return vec![p::FileInfo { id: 0, path: main_path.to_string(), role: p::FileRole::Main }];
        }
        self.files
            .iter()
            .enumerate()
            .map(|(i, f)| p::FileInfo { id: i as u32, path: f.path.clone(), role: f.role })
            .collect()
    }
}

pub fn v3(v: DVec3) -> p::Vec3 {
    p::Vec3(v.to_array())
}

pub fn m4(m: &DMat4) -> p::Mat4 {
    p::Mat4(m.to_cols_array())
}

pub fn value(v: &PValue) -> p::Value {
    match v {
        PValue::Undef => p::Value::Undef,
        PValue::Bool(b) => p::Value::Bool(*b),
        PValue::Num(n) => p::Value::Num(*n),
        PValue::Str(s) => p::Value::Str(s.clone()),
        PValue::List(l) => p::Value::List(l.iter().map(value).collect()),
        PValue::Range { start, step, end } => p::Value::Range { start: *start, step: *step, end: *end },
        PValue::Function => p::Value::Opaque { opaque: "function".into() },
        PValue::Other(s) => p::Value::Opaque { opaque: s.clone() },
    }
}

pub fn pvalue(v: &p::Value) -> PValue {
    match v {
        p::Value::Undef | p::Value::Opaque { .. } => PValue::Undef,
        p::Value::Bool(b) => PValue::Bool(*b),
        p::Value::Num(n) => PValue::Num(*n),
        p::Value::Str(s) => PValue::Str(s.clone()),
        p::Value::List(l) => PValue::List(l.iter().map(pvalue).collect()),
        p::Value::Range { start, step, end } => PValue::Range { start: *start, step: *step, end: *end },
    }
}

/// Source text of a value (for "Als Standard übernehmen").
pub fn value_text(v: &p::Value) -> String {
    match v {
        p::Value::Undef | p::Value::Opaque { .. } => "undef".into(),
        p::Value::Bool(b) => b.to_string(),
        p::Value::Num(n) => cad_eval::fmt_literal(*n, 0.0),
        p::Value::Str(s) => format!("{s:?}"),
        p::Value::List(l) => format!("[{}]", l.iter().map(value_text).collect::<Vec<_>>().join(", ")),
        p::Value::Range { start, step, end } => format!(
            "[{} : {} : {}]",
            cad_eval::fmt_literal(*start, 0.0),
            cad_eval::fmt_literal(*step, 0.0),
            cad_eval::fmt_literal(*end, 0.0)
        ),
    }
}

pub fn stmt(s: u32) -> Option<u32> {
    (s != NO_STMT).then_some(s)
}

pub fn severity(s: cad_syntax::Severity) -> p::Severity {
    match s {
        cad_syntax::Severity::Error => p::Severity::Error,
        cad_syntax::Severity::Warning => p::Severity::Warning,
        cad_syntax::Severity::Info => p::Severity::Info,
    }
}

pub fn engine_severity(s: cad_engine::Severity) -> p::Severity {
    match s {
        cad_engine::Severity::Error => p::Severity::Error,
        cad_engine::Severity::Warning => p::Severity::Warning,
    }
}

pub fn lang_diagnostics(src: &Source, diags: &[cad_syntax::Diagnostic], source: p::DiagSource) -> Vec<p::Diagnostic> {
    diags
        .iter()
        .map(|d| p::Diagnostic {
            severity: severity(d.severity),
            message: d.message.clone(),
            range: src.range(d.span),
            source,
            node: None,
            file: src.path(d.span),
            at: None,
            nodes: None,
        })
        .collect()
}

pub fn kernel_diagnostics<S>(src: &Source, ev: &Evaluation<S>) -> Vec<p::Diagnostic> {
    ev.diagnostics()
        .map(|d| p::Diagnostic {
            severity: engine_severity(d.severity),
            message: diag_text(src, &ev.tree, d),
            range: src.range(d.span),
            source: p::DiagSource::Kernel,
            node: Some(d.node),
            file: src.path(d.span),
            at: d.at.map(v3),
            nodes: (!d.nodes.is_empty()).then(|| d.nodes.clone()),
        })
        .collect()
}

/// Name of what node `n` places ([`cad_engine::object_label`]).
pub fn object_label(tree: &Tree, n: NodeIdx) -> String {
    cad_engine::object_label(tree, n)
}

/// Text of a kernel diagnostic: the operands it names (`{nodes}`) as "line 12 (dom)" (with
/// the file for other project files), otherwise the engine's text.
pub fn diag_text(src: &Source, tree: &Tree, d: &cad_engine::Diagnostic) -> String {
    let Some(t) = &d.template else { return d.message.clone() };
    let names: Vec<String> = d
        .nodes
        .iter()
        .map(|&n| {
            let node = tree.node(n);
            let label = object_label(tree, n);
            let r = src.range(node.span);
            match src.path(node.span) {
                Some(f) => cad_i18n::tr!("{f} Zeile {} ({label})", "{f} line {} ({label})", r.line),
                None => cad_i18n::tr!("Zeile {} ({label})", "line {} ({label})", r.line),
            }
        })
        .collect();
    t.replace("{nodes}", &cad_engine::operand_list(&names))
}

fn polylines(v: &[Vec<DVec3>]) -> Vec<Vec<p::Vec3>> {
    v.iter().map(|l| l.iter().map(|&q| v3(q)).collect()).collect()
}

fn dim(d: cad_ir::Dim) -> p::Dim {
    match d {
        cad_ir::Dim::Two => p::Dim::Two,
        cad_ir::Dim::Three => p::Dim::Three,
    }
}

/// Non-first solid child of a `difference`.
pub fn is_subtracted(tree: &Tree, n: u32) -> bool {
    let Some(par) = tree.node(n).parent else { return false };
    if !matches!(tree.node(par).op, Op::Boolean { op: BoolOp::Difference, .. }) {
        return false;
    }
    let f = tree.node(n).flags;
    if f.disabled || f.background || f.zone {
        return false;
    }
    tree.solid_children(par).next() != Some(n)
}

fn select(s: cad_ir::Select) -> p::SelectMode {
    match s {
        cad_ir::Select::Inside => p::SelectMode::Inside,
        cad_ir::Select::Touch => p::SelectMode::Touch,
    }
}

/// Parameters the view needs to draw zone blocks, profile operations and surfaces.
pub fn op_info(op: &Op) -> Option<p::OpInfo> {
    Some(match op {
        Op::Blend { kind, r, select: s, .. } => {
            p::OpInfo::Blend { chamfer: *kind == cad_ir::BlendKind::Chamfer, r: *r, select: select(*s) }
        }
        Op::Shell { t, select: s } => p::OpInfo::Shell { t: *t, select: select(*s) },
        Op::Zone(z) => match *z {
            cad_ir::ZoneShape::Line { point, dir, d } => {
                p::OpInfo::Zone { line: true, point: v3(point), dir: v3(dir), d }
            }
            cad_ir::ZoneShape::Plane { point, normal, d } => {
                p::OpInfo::Zone { line: false, point: v3(point), dir: v3(normal), d }
            }
        },
        Op::LinearExtrude { h, twist, scale, center } => {
            p::OpInfo::LinearExtrude { h: *h, center: *center, twist: *twist, scale: scale.to_array() }
        }
        Op::RotateExtrude { angle } => p::OpInfo::RotateExtrude { angle: *angle },
        Op::Loft { ruled } => p::OpInfo::Loft { ruled: *ruled },
        Op::Sweep { path, interp } => p::OpInfo::Sweep {
            path: path.iter().map(|&q| v3(q)).collect(),
            spline: *interp == cad_kernel::PathInterp::Spline,
        },
        Op::Surface { rows } => {
            p::OpInfo::Surface { rows: rows.iter().map(|r| r.iter().map(|&q| v3(q)).collect()).collect() }
        }
        Op::Thicken { t } => p::OpInfo::Thicken { t: *t },
        Op::Blend2 { kind, r, select: s } => {
            p::OpInfo::Blend2d { chamfer: *kind == cad_ir::BlendKind::Chamfer, r: *r, select: select(*s) }
        }
        Op::Offset { round, d, chamfer } => p::OpInfo::Offset { round: *round, d: *d, chamfer: *chamfer },
        Op::Stroke { w, cap, join } => p::OpInfo::Stroke {
            w: *w,
            cap: format!("{cap:?}").to_lowercase(),
            join: format!("{join:?}").to_lowercase(),
        },
        _ => return None,
    })
}

/// Display data of a sketch node: its profile with the segment statements of the evaluation.
pub fn sketch_info(out: &EvalOutput, node: u32, op: &Op, src: &Source, root_inv: &DMat4) -> Option<p::OpInfo> {
    let Op::Profile(cad_ir::Profile::Sketch { contours, closed }) = op else { return None };
    let info = out.sketches.iter().find(|s| s.node == node)?;
    // display tolerance relative to the sketch size
    let mut size: f64 = 1e-6;
    for c in contours {
        for g in &c.segs {
            size = size.max((g.end() - g.start()).length());
        }
    }
    let tol = (size * 1e-3).clamp(1e-4, 0.05);
    let p2 = |q: glam::DVec2| v3(q.extend(0.0));
    let contours = contours
        .iter()
        .zip(&info.contours)
        .map(|(c, ci)| p::SketchContour {
            hole: c.hole,
            closed: c.closed,
            start: c.segs.first().map(|g| p2(g.start())).unwrap_or(p::Vec3([0.0; 3])),
            start_range: ci.start_span.map(|s| src.range(s)),
            segments: c
                .segs
                .iter()
                .zip(&ci.segs)
                .map(|(g, gi)| {
                    let points = g.to_kernel().map(|k| k.sample(tol)).unwrap_or_default().into_iter().map(p2).collect();
                    let ctrl = match g {
                        cad_ir::Seg::Bezier { points } => points.iter().map(|q| p2(*q)).collect(),
                        cad_ir::Seg::Arc { center, .. } => vec![p2(*center)],
                        _ => Vec::new(),
                    };
                    p::SketchSegment {
                        kind: g.kind().into(),
                        stmt: gi.stmt,
                        range: src.range(gi.span),
                        points,
                        ctrl,
                        implicit: gi.implicit,
                        error: gi.error,
                    }
                })
                .collect(),
        })
        .collect();
    let plane = m4(&(*root_inv * cad_eval::sketch_edit::sketch_frame(&out.tree, node)));
    Some(p::OpInfo::Sketch { closed: *closed, contours, plane })
}

/// Mesh key of a node: hex of its content key.
pub fn mesh_key(k: &cad_engine::CacheKey) -> String {
    k.to_hex()
}

/// Per-node data available from the engine (complete scene) or from the started job
/// (preliminary scene: keys only).
pub enum KernelInfo<'a, S> {
    Done(&'a Evaluation<S>),
    Pending { keys: &'a [cad_engine::CacheKey], tags: &'a [u32] },
}

pub struct SceneInput<'a, S> {
    pub version: u64,
    pub src: &'a Source,
    pub out: &'a EvalOutput,
    pub kernel: KernelInfo<'a, S>,
    pub stats: p::Stats,
    /// Project files, include statements and part volumes (`None`: single source).
    pub project: Option<ProjectInfo<'a>>,
}

/// Project data for a scene.
pub struct ProjectInfo<'a> {
    pub main_path: &'a str,
    pub proj: &'a cad_eval::Project,
    /// Volume per part (same order as `EvalOutput::parts`).
    pub part_volumes: &'a [Option<f64>],
}

/// `include`/`use` statements of all project files.
pub fn include_refs(src: &Source, proj: &cad_eval::Project) -> Vec<p::IncludeRef> {
    use cad_syntax::ast::StmtKind;
    let mut out = Vec::new();
    let mut visit = |stmts: &[cad_syntax::ast::Stmt]| {
        for s in stmts {
            s.walk(&mut |s| {
                if let StmtKind::Include { path, path_span, is_use } = &s.kind {
                    let from = proj.path_of(*path_span);
                    let target = cad_eval::resolve_path(from, path);
                    let found = proj.files.iter().any(|f| f.path == target);
                    out.push(p::IncludeRef { range: src.range(*path_span), target, is_use: *is_use, found });
                }
            });
        }
    };
    visit(&proj.main.stmts);
    for l in &proj.libs {
        visit(&l.ast.stmts);
    }
    out.sort_by_key(|r| (r.range.file, r.range.from));
    out.dedup();
    out
}

/// Colour table of a tree with the palette's default colour and names (Plan 16.3).
pub fn color_table(tree: &cad_ir::Tree) -> cad_engine::colors::ColorTable {
    use cad_eval::palette;
    cad_engine::colors::ColorTable::of(tree, palette::rgba_of(palette::DEFAULT_RGB), &|n| {
        palette::palette_color(n).is_some()
    })
}

/// Protocol form of a colour table.
pub fn color_infos(t: &cad_engine::colors::ColorTable, tree: &cad_ir::Tree, src: &Source) -> Vec<p::ColorInfo> {
    use cad_engine::colors::{ColorSource as S, key_hex};
    t.entries
        .iter()
        .map(|e| {
            let mut spans: Vec<Span> = Vec::new();
            for &n in &e.nodes {
                let sp = tree.node(n).span;
                if !spans.contains(&sp) {
                    spans.push(sp);
                }
            }
            p::ColorInfo {
                key: key_hex(e.key),
                rgba: p::Rgba(e.rgba),
                label: e.label.clone(),
                source: match e.source {
                    S::Default => p::ColorSource::Default,
                    S::Palette => p::ColorSource::Palette,
                    S::Random => p::ColorSource::Random,
                    S::Exact => p::ColorSource::Exact,
                },
                filament: e.filament,
                explicit: e.explicit,
                ranges: spans.into_iter().map(|s| src.range(s)).collect(),
                nodes: e.nodes.clone(),
            }
        })
        .collect()
}

/// World bounds of node-system bounds under `m`.
fn world_bounds(lo: DVec3, hi: DVec3, m: &DMat4) -> p::Bounds {
    let mut wlo = DVec3::splat(f64::INFINITY);
    let mut whi = DVec3::splat(f64::NEG_INFINITY);
    for i in 0..8 {
        let c = DVec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let w = m.transform_point3(c);
        wlo = wlo.min(w);
        whi = whi.max(w);
    }
    p::Bounds { min: v3(wlo), max: v3(whi) }
}

pub fn scene<S>(i: SceneInput<'_, S>) -> p::Scene {
    let SceneInput { version, src, out, kernel, stats, project } = i;
    let tree = &out.tree;
    let r = |s: Span| src.range(s);
    let ro = |s: Option<Span>| s.map(|s| src.range(s));

    let root_inv = tree.root.map(|n| tree.node(n).world.inverse()).unwrap_or(DMat4::IDENTITY);
    let nodes = tree
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let idx = i as u32;
            let (op_instance, label) = match &n.op {
                Op::ModuleCall { name, instance } => (instance.clone(), name.clone()),
                o => (None, tree.assert_of(idx).map_or_else(|| o.label(), |a| a.kind.name().to_string())),
            };
            let base = p::SceneNode {
                idx,
                parent: n.parent,
                children: n.children.clone(),
                stmt: stmt(n.stmt),
                range: r(n.span),
                label,
                instance: op_instance,
                flags: p::NodeFlags {
                    highlight: n.flags.highlight,
                    background: n.flags.background,
                    disabled: n.flags.disabled,
                    zone: n.flags.zone,
                    keepout: n.flags.keepout,
                },
                subtracted: is_subtracted(tree, idx),
                color: n.color.map(p::Rgba),
                world: m4(&n.world),
                display_world: m4(&(root_inv * n.world)),
                module_inst: n.module_inst,
                inst_key: n.inst_key,
                status: p::NodeStatus::Pending,
                messages: Vec::new(),
                hints: Vec::new(),
                selected_edges: Vec::new(),
                guides: Vec::new(),
                opened_faces: 0,
                blend_suspended: false,
                bounds: None,
                dim: tree.dim(idx).map(dim),
                mesh_key: None,
                tag: 0,
                cached: false,
                tol: n.tol,
                op_info: match &n.op {
                    Op::Profile(cad_ir::Profile::Sketch { .. }) => sketch_info(out, idx, &n.op, src, &root_inv),
                    op => op_info(op),
                },
            };
            match &kernel {
                KernelInfo::Pending { keys, tags } => p::SceneNode {
                    mesh_key: keys.get(i).map(mesh_key),
                    tag: tags.get(i).copied().unwrap_or(0),
                    status: if n.flags.disabled { p::NodeStatus::Skipped } else { p::NodeStatus::Pending },
                    ..base
                },
                KernelInfo::Done(ev) => {
                    let rep = ev.report(idx);
                    let status = match rep.status {
                        Status::Ok => p::NodeStatus::Ok,
                        Status::Warning => p::NodeStatus::Warning,
                        Status::Error => p::NodeStatus::Error,
                        Status::Skipped => p::NodeStatus::Skipped,
                    };
                    p::SceneNode {
                        status,
                        messages: rep
                            .diagnostics
                            .iter()
                            .map(|d| p::NodeMessage {
                                severity: engine_severity(d.severity),
                                message: diag_text(src, &ev.tree, d),
                                edges: polylines(&d.edges),
                                at: d.at.map(|q| v3(rep.display_world.inverse().transform_point3(q))),
                            })
                            .collect(),
                        hints: rep.hints.clone(),
                        selected_edges: polylines(&rep.selected_edges),
                        guides: rep
                            .guides
                            .iter()
                            .map(|(k, pts)| p::GuideLine {
                                kind: k.clone(),
                                points: pts.iter().map(|&q| v3(q)).collect(),
                            })
                            .collect(),
                        opened_faces: rep.opened_faces as u32,
                        blend_suspended: rep.blend_suspended,
                        bounds: rep.bounds.map(|(a, b)| p::Bounds { min: v3(a), max: v3(b) }),
                        dim: rep.dim.map(dim).or(base.dim),
                        mesh_key: (status != p::NodeStatus::Skipped).then(|| mesh_key(&rep.key)),
                        tag: rep.tag,
                        cached: rep.cached,
                        display_world: m4(&rep.display_world),
                        ..base
                    }
                }
            }
        })
        .collect();

    let tag_nodes = match &kernel {
        KernelInfo::Done(ev) => ev
            .tag_nodes
            .iter()
            .map(|(&tag, refs)| p::TagNodes { tag, nodes: refs.iter().map(|n| n.node).collect() })
            .collect(),
        KernelInfo::Pending { tags, .. } => {
            let mut m: std::collections::BTreeMap<u32, Vec<u32>> = Default::default();
            for (i, &t) in tags.iter().enumerate() {
                if !tree.nodes[i].flags.disabled {
                    m.entry(t).or_default().push(i as u32);
                }
            }
            m.into_iter().map(|(tag, nodes)| p::TagNodes { tag, nodes }).collect()
        }
    };

    let grids = out
        .grids
        .iter()
        .map(|g| p::Grid {
            id: g.id,
            name: g.name.clone(),
            stmt: g.stmt,
            range: r(g.span),
            name_range: r(g.name_span),
            module_inst: g.module_inst,
            in_context: g.in_context,
            frame: m4(&g.frame),
            frame_module: m4(&g.frame_module),
            planes: g
                .planes
                .iter()
                .map(|pl| p::Plane {
                    axis: pl.axis,
                    name: pl.name.clone(),
                    base: pl.base.clone(),
                    index: pl.index,
                    value: pl.value,
                    value_range: ro(pl.value_span),
                    literal: ro(pl.literal),
                    is_ref: pl.origin.is_ref(),
                    implicit: pl.implicit,
                })
                .collect(),
            visible: r(g.visible),
        })
        .collect();

    let mut anchors = Vec::new();
    let instances = out
        .instances
        .iter()
        .map(|inst| {
            // Anchors are in instance coordinates; `world` maps them into the context.
            for (k, a) in inst.anchors.iter().enumerate() {
                anchors.push(p::Anchor {
                    inst: inst.id,
                    index: k as u32,
                    name: a.name.clone(),
                    instance: inst.label.clone(),
                    text: inst.label.as_ref().map(|l| format!("{l}.{}", a.name)),
                    stmt: a.stmt,
                    range: r(a.span),
                    point: v3(inst.world.transform_point3(a.point)),
                    dir: a.dir.map(|d| v3(inst.world.transform_vector3(d).normalize_or_zero())),
                });
            }
            p::Instance {
                id: inst.id,
                module: inst.module.clone(),
                label: inst.label.clone(),
                call_stmt: inst.call_stmt,
                call_range: ro(inst.call_span),
                def_stmt: inst.def_stmt,
                def_range: ro(inst.def_span),
                node: inst.node,
                parent: inst.parent,
                matrix: m4(&inst.matrix),
                world: m4(&inst.world),
                anchors: inst
                    .anchors
                    .iter()
                    .map(|a| p::LocalAnchor {
                        name: a.name.clone(),
                        stmt: a.stmt,
                        range: r(a.span),
                        point: v3(a.point),
                        dir: a.dir.map(v3),
                    })
                    .collect(),
                visible: ro(inst.visible),
                depth: inst.depth,
            }
        })
        .collect();

    let handle = |h: &cad_eval::Handle| p::Handle {
        id: h.id,
        stmt: h.stmt,
        range: r(h.span),
        kind: match h.kind {
            cad_eval::HandleKind::Point => p::HandleKind::Point,
            cad_eval::HandleKind::Radius => p::HandleKind::Radius,
            cad_eval::HandleKind::Height => p::HandleKind::Height,
            cad_eval::HandleKind::Angle { axis } => p::HandleKind::Angle { axis },
            cad_eval::HandleKind::Plane { axis } => p::HandleKind::Plane { axis },
            cad_eval::HandleKind::Scale { axis } => p::HandleKind::Scale { axis },
        },
        index: h.index,
        frame: m4(&h.frame),
        pos: v3(h.pos),
        world_pos: v3(h.frame.transform_point3(h.pos)),
        axis: h.axis.map(v3),
        base: h.base.map(v3),
        value: h.value.clone(),
        comps: h.comps.iter().map(|c| c.map(|c| p::Comp { range: r(c.span), scale: c.scale })).collect(),
        has_ref: h.has_ref,
        expr_range: r(h.expr_span),
        def_range: ro(h.def_span),
        via_var: h.via_var,
        via_default: h.via_default,
        locked: h.locked,
        grids: h.grids.clone(),
        instances: h.instances.clone(),
        node: h.node,
        module_inst: h.module_inst,
    };
    let handles = out.handles.iter().map(handle).collect();
    let locked_handles = out.locked_handles.iter().map(handle).collect();

    let hints = out
        .hints
        .iter()
        .map(|h| p::Hint {
            stmt: h.stmt,
            range: r(h.span),
            text: h.text.clone(),
            value: value(&h.value),
            literal: ro(h.literal),
            in_context: h.in_context,
        })
        .collect();

    let modules = out
        .modules
        .iter()
        .map(|m| p::Module {
            name: m.name.clone(),
            stmt: m.stmt,
            range: r(m.span),
            name_range: r(m.name_span),
            body_range: r(m.body_span),
            path: m.path.clone(),
            params: m
                .params
                .iter()
                .map(|pa| p::Param {
                    name: pa.name.clone(),
                    range: r(pa.span),
                    default_text: pa.default_text.clone(),
                    default_range: ro(pa.default_span),
                    default_value: pa.default_value.as_ref().map(value),
                    ty: match pa.ty {
                        cad_eval::ParamType::Number => p::ParamType::Number,
                        cad_eval::ParamType::Bool => p::ParamType::Bool,
                        cad_eval::ParamType::Vector => p::ParamType::Vector,
                        cad_eval::ParamType::String => p::ParamType::String,
                        cad_eval::ParamType::List => p::ParamType::List,
                        cad_eval::ParamType::Unknown => p::ParamType::Unknown,
                    },
                })
                .collect(),
        })
        .collect();

    let sel = |s: &cad_eval::value::Sel| match *s {
        cad_eval::value::Sel::Plane(plane) => p::Sel::Plane { plane },
        cad_eval::value::Sel::Wild => p::Sel::Wild,
        cad_eval::value::Sel::Coord(value) => p::Sel::Coord { value },
    };
    let refs = out
        .refs
        .iter()
        .map(|rf| p::Ref {
            range: r(rf.span),
            stmt: rf.stmt,
            target: match &rf.target {
                cad_eval::RefTarget::Grid { grid, sel: s } => {
                    p::RefTarget::Grid { grid: *grid, sel: [sel(&s[0]), sel(&s[1]), sel(&s[2])] }
                }
                cad_eval::RefTarget::Plane { grid, plane } => p::RefTarget::Plane { grid: *grid, plane: *plane },
                cad_eval::RefTarget::Anchor { inst, anchor } => p::RefTarget::Anchor { inst: *inst, anchor: *anchor },
            },
        })
        .collect();

    let echoes = out
        .echoes
        .iter()
        .map(|e| p::Echo { stmt: e.stmt, range: r(e.span), text: e.text.clone(), in_context: e.in_context })
        .collect();

    let c = &out.context;
    let context = p::ContextInfo {
        module: c.module.clone(),
        def_stmt: c.def_stmt,
        def_range: ro(c.def_span),
        path: c.path.clone(),
        params: c
            .params
            .iter()
            .map(|pv| p::ParamValue { name: pv.name.clone(), value: value(&pv.value), overridden: pv.overridden })
            .collect(),
        root_modifier: c.root_modifier,
    };

    let parts = out
        .parts
        .iter()
        .enumerate()
        .map(|(k, pt)| {
            let (bounds, volume) = match &kernel {
                KernelInfo::Done(ev) => {
                    let rep = ev.report(pt.node);
                    let b = rep.bounds.map(|(lo, hi)| world_bounds(lo, hi, &rep.display_world));
                    (b, project.as_ref().and_then(|pi| pi.part_volumes.get(k).copied().flatten()))
                }
                KernelInfo::Pending { .. } => (None, None),
            };
            p::Part { name: pt.name.clone(), node: pt.node, inst: pt.inst, range: r(pt.span), bounds, volume }
        })
        .collect();
    let (files, includes) = match &project {
        Some(pi) => (src.file_infos(pi.main_path), include_refs(src, pi.proj)),
        None => (src.file_infos(cad_eval::DEFAULT_MAIN), Vec::new()),
    };

    let root_is_subtree = tree.root.is_some_and(|r| tree.node(r).parent.is_some());
    let colors = color_infos(&color_table(tree), tree, src);
    p::Scene {
        version,
        files,
        includes,
        parts,
        colors,

        complete: matches!(kernel, KernelInfo::Done(_)),
        context,
        root: tree.root,
        root_is_subtree,
        nodes,
        grids,
        anchors,
        instances,
        handles,
        locked_handles,
        hints,
        modules,
        refs,
        tag_nodes,
        echoes,
        aborted: out.aborted,
        asserts: Vec::new(),
        stats,
        render: tree.render.as_ref().map(|d| render_block(d, src)),
    }
}

/// A change of the render block as the language's edit (values as literal text).
pub fn render_edit(c: &p::RenderChange) -> cad_eval::RenderEdit {
    use cad_eval::{EditTarget, RenderEdit as E};
    let val = |v: &p::Value| (!matches!(v, p::Value::Undef)).then(|| value_text(v));
    let target = |t: &p::RenderTarget| match t {
        p::RenderTarget::Color { color } => EditTarget::Color(color.clone()),
        p::RenderTarget::Part { part } => EditTarget::Part(part.clone()),
    };
    match c {
        p::RenderChange::Set { key, value } => E::Set { key: key.clone(), value: val(value) },
        p::RenderChange::Light { name, key, value } => {
            E::Light { name: name.clone(), key: key.clone(), value: val(value) }
        }
        p::RenderChange::RemoveLight { name } => E::RemoveLight { name: name.clone() },
        p::RenderChange::Material { target: t, key, value } => {
            E::Material { target: target(t), key: key.clone(), value: val(value) }
        }
        p::RenderChange::RemoveMaterial { target: t } => E::RemoveMaterial { target: target(t) },
    }
}

/// The evaluated `render { … }` block for the UI and the CLI.
pub fn render_block(d: &cad_ir::RenderDecl, src: &Source) -> p::RenderBlock {
    let hex = |c: [f32; 4]| cad_eval::palette::hex_string([c[0], c[1], c[2], 1.0]);
    p::RenderBlock {
        range: src.range(d.span),
        style: d.style.clone(),
        edges: d.edges,
        environment: d.environment.clone(),
        ambient: d.ambient,
        light_intensity: d.light_intensity,
        shadows: d.shadows,
        shadow_softness: d.shadow_softness,
        ao: d.ao,
        ao_radius: d.ao_radius,
        ao_strength: d.ao_strength,
        ground: d.ground,
        ground_color: d.ground_color.map(hex),
        background: d.background.as_ref().map(|v| v.iter().map(|c| hex(*c)).collect()),
        lights: d
            .lights
            .iter()
            .map(|l| p::RenderLight {
                name: l.name.clone(),
                azimuth: l.azimuth,
                elevation: l.elevation,
                intensity: l.intensity,
                color: hex(l.color),
                range: src.range(l.span),
            })
            .collect(),
        materials: d
            .materials
            .iter()
            .map(|m| p::RenderMaterial {
                target: match &m.target {
                    cad_ir::RenderTarget::Color { label, .. } => p::RenderTarget::Color { color: label.clone() },
                    cad_ir::RenderTarget::Part(n) => p::RenderTarget::Part { part: n.clone() },
                },
                preset: m.preset.clone(),
                metalness: m.metalness,
                roughness: m.roughness,
                clearcoat: m.clearcoat,
                range: src.range(m.span),
            })
            .collect(),
    }
}
