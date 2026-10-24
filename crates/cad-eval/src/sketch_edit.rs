//! Sketch editor (Plan 17.6, umgesetzt in 17.12): drawing in the plane of a `sketch { … }` block.
//!
//! Everything the editor needs besides the mouse is computed here, on the evaluation and the
//! project: which sketch the cursor is in and where new statements go ([`place`]), the drawing
//! state there (current point and direction), snapping ([`snap`]: grid points, anchors, end,
//! centre, mid and intersection points, tangent, horizontal/vertical, grid lines, step), the
//! code of every tool action ([`commit`]), previews ([`preview`]), deleting a segment
//! ([`delete_segment`]) and the drawing raster ([`raster`]). The code stays the only source:
//! every action is a [`TextEdit`] of the main file.

use cad_ir::{NodeIdx, Op, Profile, Seg, Tree};
use cad_kernel::Segment;
use cad_kernel::region2::{CornerKind, round_corner};
use cad_syntax::Span;
use cad_syntax::ast::{Stmt, StmtKind};
use glam::{DMat4, DVec2, DVec3};
use serde::{Deserialize, Serialize};

use crate::edit::span_visible;
use crate::output::{EvalOutput, SketchInfo, SketchStep, SketchStepKind, TextEdit};
use crate::project::Project;
use crate::value::{fmt_literal, fmt_num};

/// Tools of the sketch editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SketchTool {
    /// Pick a segment (hover), no drawing.
    Select,
    /// `start(p); line(to = …);` – a chain of lines.
    Line,
    /// Arc through three points: `arc(through = q, to = p)`.
    ArcThrough,
    /// Arc around a centre: `arc(center = c, angle = a)` (or `to = p` on a snapped point).
    ArcCenter,
    /// Tangent arc continuing the contour: `arc(to = p)`.
    ArcTangent,
    /// Cubic Bézier curve: `bezier(c1 = …, c2 = …, to = …)`.
    Bezier,
    /// Rectangle from two corners: a closed contour of lines.
    Rect,
    /// Circle from centre and a point on it: `start(p); arc(center = c, angle = 360); close();`.
    Circle,
    /// Round a vertex: `corner(r = …)` after the segment ending there.
    Corner,
    /// `close();` for the open contour at the insertion point.
    Close,
    /// Removes the segment statement at the cursor.
    Delete,
}

/// What a point snapped to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapKind {
    /// Grid point (`g[a, b]`).
    Grid,
    /// Anchor of a named instance (`k.ecke`).
    Anchor,
    /// Start point of the open contour (a click closes it).
    Start,
    /// End point of a segment (or start of a contour).
    End,
    /// Centre of an arc.
    Center,
    /// Intersection of two segments.
    Intersection,
    /// Midpoint of a line or an arc.
    Mid,
    /// Vertex between two segments (corner tool).
    Vertex,
    /// Along the end tangent of the contour (`line(length = …)`).
    Tangent,
    /// Same y as the previous point.
    Horizontal,
    /// Same x as the previous point.
    Vertical,
    /// On a grid plane (one coordinate `g.name`).
    GridLine,
    /// Numbers rounded to the step (also Alt).
    Step,
}

impl SnapKind {
    pub fn name(self) -> &'static str {
        match self {
            SnapKind::Grid => "grid",
            SnapKind::Anchor => "anchor",
            SnapKind::Start => "start",
            SnapKind::End => "end",
            SnapKind::Center => "center",
            SnapKind::Intersection => "intersection",
            SnapKind::Mid => "mid",
            SnapKind::Vertex => "vertex",
            SnapKind::Tangent => "tangent",
            SnapKind::Horizontal => "horizontal",
            SnapKind::Vertical => "vertical",
            SnapKind::GridLine => "grid_line",
            SnapKind::Step => "step",
        }
    }

    pub fn from_name(s: &str) -> Option<SnapKind> {
        use SnapKind::*;
        [Grid, Anchor, Start, End, Center, Intersection, Mid, Vertex, Tangent, Horizontal, Vertical, GridLine, Step]
            .into_iter()
            .find(|k| k.name() == s)
    }

    /// A snap onto a point of the model (not a free position).
    pub fn is_point(self) -> bool {
        matches!(
            self,
            SnapKind::Grid
                | SnapKind::Anchor
                | SnapKind::Start
                | SnapKind::End
                | SnapKind::Center
                | SnapKind::Intersection
                | SnapKind::Mid
                | SnapKind::Vertex
        )
    }

    /// Order among point snaps at the same distance (smaller first).
    fn rank(self) -> u8 {
        match self {
            SnapKind::Grid => 0,
            SnapKind::Anchor => 1,
            SnapKind::Start => 2,
            SnapKind::Vertex => 3,
            SnapKind::End => 4,
            SnapKind::Center => 5,
            SnapKind::Intersection => 6,
            SnapKind::Mid => 7,
            _ => 9,
        }
    }

    /// German label for the status line.
    pub fn label(self) -> &'static str {
        match self {
            SnapKind::Grid => "Rasterpunkt",
            SnapKind::Anchor => "Anker",
            SnapKind::Start => "Anfang (schließt)",
            SnapKind::End => "Endpunkt",
            SnapKind::Center => "Mittelpunkt",
            SnapKind::Intersection => "Schnittpunkt",
            SnapKind::Mid => "Mitte",
            SnapKind::Vertex => "Ecke",
            SnapKind::Tangent => "tangential",
            SnapKind::Horizontal => "horizontal",
            SnapKind::Vertical => "vertikal",
            SnapKind::GridLine => "Rasterlinie",
            SnapKind::Step => "Schrittweite",
        }
    }
}

/// Grid point of a pick: grid id and plane names (z: `None` for the implicit plane, written
/// as `g[x, y]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridSel {
    pub grid: u32,
    pub x: String,
    pub y: String,
    pub z: Option<String>,
}

/// A picked point of a tool: position in the sketch plane and the expression written for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pick {
    pub p: DVec2,
    pub text: String,
    pub kind: SnapKind,
    pub grid: Option<GridSel>,
}

impl Pick {
    fn num(p: DVec2, kind: SnapKind) -> Pick {
        Pick { p, text: point_text(p), kind, grid: None }
    }
}

/// Result of snapping a mouse position.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapped {
    pub pick: Pick,
    /// Display text (the reference, the kind or the coordinates).
    pub label: String,
    /// Help lines in the sketch plane (horizontal/vertical, tangent).
    pub guides: Vec<Vec<DVec2>>,
}

/// Open contour at the insertion point.
#[derive(Debug, Clone, PartialEq)]
pub struct Cur {
    pub start: DVec2,
    pub end: DVec2,
    pub tangent: Option<DVec2>,
    /// Segments of the contour so far.
    pub segs: usize,
    pub contour: u32,
    /// The contour has statements after the insertion point.
    pub continues: bool,
}

/// Where new statements go: inserted at `at` as `before + s1 + sep + s2 … + after`.
#[derive(Debug, Clone, PartialEq)]
struct Ins {
    at: u32,
    before: String,
    sep: String,
    after: String,
    /// The statements go into a new `sketch { … }` block (indent of its body).
    block: Option<String>,
}

/// A `{ … }` block of the main file: the sketch block, a `hole` block or a module body.
#[derive(Debug, Clone)]
struct Blk<'a> {
    children: &'a [Stmt],
    open: u32,
    close: u32,
    /// Indentation of the statements in the block.
    indent: String,
    /// Indentation of the line with the closing brace.
    base: String,
}

/// Sketch and insertion point for the editor cursor.
#[derive(Debug, Clone)]
pub struct Place<'o> {
    /// The sketch the cursor is in (`None`: the first action creates one).
    pub info: Option<&'o SketchInfo>,
    /// Sketch plane → context root (tree root system).
    pub frame: DMat4,
    /// Open contour at the insertion point.
    pub cur: Option<Cur>,
    /// Source position the insertion stands for (statements before it count).
    pub pos: u32,
    /// Grids and named instances visible at the insertion point.
    pub grids: Vec<u32>,
    pub insts: Vec<u32>,
    ins: Ins,
    /// Insertion for a new contour (after the end of a contour that continues).
    fresh: Ins,
    /// Containers of the sketch (sketch block, `hole` blocks) for deletes and corners.
    blocks: Vec<(Span, Blk<'o>)>,
}

/// rotate_extrude: profile xy → xz (x → x, y → z), as the engine maps it.
const REVOLVE: DMat4 = DMat4::from_cols_array(&[1., 0., 0., 0., 0., 0., 1., 0., 0., -1., 0., 0., 0., 0., 0., 1.]);

/// Frame of a sketch's plane in the tree root system: the node's system, for the profile of a
/// `rotate_extrude` the xz-plane (where the profile lies in the body), for a centred
/// `linear_extrude` the bottom.
pub fn sketch_frame(tree: &Tree, node: NodeIdx) -> DMat4 {
    frame_in(&tree.nodes, node)
}

/// [`sketch_frame`] on the nodes of a tree being built.
pub(crate) fn frame_in(nodes: &[cad_ir::Node], node: NodeIdx) -> DMat4 {
    let n = &nodes[node as usize];
    let mut cur = n.parent;
    let mut depth = 0;
    while let Some(a) = cur {
        let Some(an) = nodes.get(a as usize) else { break };
        match &an.op {
            Op::RotateExtrude { .. } => return an.world * REVOLVE * an.world.inverse() * n.world,
            Op::LinearExtrude { center: true, h, .. } => {
                return an.world
                    * DMat4::from_translation(DVec3::new(0.0, 0.0, -h / 2.0))
                    * an.world.inverse()
                    * n.world;
            }
            Op::LinearExtrude { .. } | Op::Loft { .. } | Op::Sweep { .. } => break,
            _ => {}
        }
        cur = an.parent;
        depth += 1;
        if depth > 256 {
            break;
        }
    }
    n.world
}

// ------------------------------------------------------------------------------------------
// text helpers
// ------------------------------------------------------------------------------------------

fn num_text(v: f64) -> String {
    let v = if v.abs() < 1e-9 { 0.0 } else { v };
    fmt_num(v)
}

fn point_text(p: DVec2) -> String {
    format!("[{}, {}]", num_text(p.x), num_text(p.y))
}

fn step_point(p: DVec2, step: f64) -> (DVec2, String) {
    let r = |v: f64| if step > 0.0 { (v / step).round() * step } else { v };
    let q = DVec2::new(r(p.x), r(p.y));
    let t = |v: f64| {
        let s = fmt_literal(v, step);
        if s == "-0" { "0".to_string() } else { s }
    };
    (q, format!("[{}, {}]", t(q.x), t(q.y)))
}

fn line_start(text: &str, pos: u32) -> u32 {
    text[..pos as usize].rfind('\n').map_or(0, |i| i as u32 + 1)
}

fn line_end(text: &str, pos: u32) -> u32 {
    text[pos as usize..].find('\n').map_or(text.len() as u32, |i| pos + i as u32)
}

fn indent_of(text: &str, pos: u32) -> String {
    let ls = line_start(text, pos) as usize;
    text[ls..].chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

/// The statement with this span (main file, anywhere).
fn find_stmt(stmts: &[Stmt], span: Span) -> Option<&Stmt> {
    let mut found = None;
    for s in stmts {
        s.walk(&mut |x| {
            if found.is_none() && x.span == span {
                found = Some(x);
            }
        });
        if found.is_some() {
            break;
        }
    }
    found
}

fn call_name(s: &Stmt) -> Option<&str> {
    match &s.kind {
        StmtKind::Call { name, .. } => Some(name.name.as_str()),
        _ => None,
    }
}

/// The `{ … }` block of a call statement with a block (`sketch { … }`, `hole { … }`).
fn block_of<'a>(text: &str, s: &'a Stmt) -> Option<Blk<'a>> {
    let StmtKind::Call { name, args, children, has_block: true } = &s.kind else { return None };
    if s.span.file() != 0 || s.span.end as usize > text.len() {
        return None;
    }
    let from = args.last().map_or(name.span.end, |a| a.span.end).max(name.span.end);
    let open = from + text[from as usize..s.span.end as usize].find('{')? as u32;
    let close = text[..s.span.end as usize].rfind('}')? as u32;
    if close <= open {
        return None;
    }
    let base = indent_of(text, close);
    let first = children.iter().find(|c| c.span.file() == 0);
    let indent = match first {
        Some(c) if text[line_start(text, c.span.start) as usize..c.span.start as usize].trim().is_empty() => {
            indent_of(text, c.span.start)
        }
        _ => format!("{}    ", indent_of(text, s.span.start)),
    };
    Some(Blk { children, open, close, indent, base })
}

/// Insertion after statement `k` (at its line end unless more code follows on the line).
fn after_stmt(text: &str, k: &Stmt, indent: &str) -> Ins {
    let e = k.span.end;
    let le = line_end(text, e);
    let rest = text[e as usize..le as usize].trim();
    let at = if rest.is_empty() || rest.starts_with("//") { le } else { e };
    Ins { at, before: format!("\n{indent}"), sep: format!("\n{indent}"), after: String::new(), block: None }
}

/// Insertion into an empty block.
fn into_empty(text: &str, b: &Blk) -> Ins {
    let ls = line_start(text, b.close);
    let sep = format!("\n{}", b.indent);
    if ls > b.open && text[ls as usize..b.close as usize].trim().is_empty() {
        Ins { at: ls, before: b.indent.clone(), sep, after: "\n".into(), block: None }
    } else {
        Ins { at: b.close, before: format!("\n{}", b.indent), sep, after: format!("\n{}", b.base), block: None }
    }
}

/// Insertion in block `b` for the cursor: after the statement on or before the cursor; on the
/// opening line, after the last statement. Returns the insertion and the position it stands for.
fn ins_in(text: &str, b: &Blk, cursor: u32) -> (Ins, u32) {
    let kids: Vec<&Stmt> = b.children.iter().filter(|c| c.span.file() == 0).collect();
    if kids.is_empty() {
        return (into_empty(text, b), b.open + 1);
    }
    let open_line_end = line_end(text, b.open);
    // statements on the cursor line count (cursor in the indentation)
    let le = line_end(text, cursor);
    let anchor = if cursor <= open_line_end || cursor > b.close {
        kids.last().copied()
    } else {
        kids.iter().rev().find(|k| k.span.start <= le).copied()
    };
    match anchor {
        Some(k) => (after_stmt(text, k, &b.indent), k.span.end),
        None => {
            let first = kids[0];
            let ls = line_start(text, first.span.start);
            let ins = if text[ls as usize..first.span.start as usize].trim().is_empty() {
                Ins {
                    at: ls,
                    before: b.indent.clone(),
                    sep: format!("\n{}", b.indent),
                    after: "\n".into(),
                    block: None,
                }
            } else {
                Ins {
                    at: first.span.start,
                    before: String::new(),
                    sep: format!("\n{}", b.indent),
                    after: format!("\n{}", b.indent),
                    block: None,
                }
            };
            (ins, b.open + 1)
        }
    }
}

impl Ins {
    /// Edit inserting `stmts`; the byte offset in its text right after the last statement.
    fn edit(&self, stmts: &[String]) -> (TextEdit, usize) {
        let body = match &self.block {
            None => stmts.join(&self.sep),
            Some(inner) => {
                let outer = self.sep.trim_start_matches('\n');
                format!("sketch {{\n{inner}{}\n{outer}}}", stmts.join(&format!("\n{inner}")))
            }
        };
        let text = format!("{}{}{}", self.before, body, self.after);
        let end = match &self.block {
            None => self.before.len() + body.len(),
            Some(_) => self.before.len() + body.rfind('\n').unwrap_or(body.len()),
        };
        (TextEdit { span: Span { start: self.at, end: self.at }, text }, end)
    }
}

// ------------------------------------------------------------------------------------------
// placement
// ------------------------------------------------------------------------------------------

/// Is a step's statement inside the sketch block (main file)?
fn inside(s: &SketchStep, block: Span) -> bool {
    s.span.file() == 0 && block.start <= s.span.start && s.span.end <= block.end
}

/// Index of the last step before position `pos` (steps of statements outside the block, from
/// module calls, count where they were evaluated).
fn last_step_before(info: &SketchInfo, pos: u32) -> Option<usize> {
    let next = info.steps.iter().position(|s| inside(s, info.span) && s.span.start >= pos);
    match next {
        Some(0) => None,
        Some(j) => Some(j - 1),
        None => info.steps.len().checked_sub(1),
    }
}

fn cur_at(info: &SketchInfo, pos: u32) -> Option<Cur> {
    let i = last_step_before(info, pos)?;
    let st = &info.steps[i];
    if st.kind == SketchStepKind::Close {
        return None;
    }
    let contour = st.contour;
    let start = info.steps[..=i].iter().rev().find(|s| s.contour == contour && s.kind == SketchStepKind::Start)?.point;
    let segs = info.steps[..=i].iter().filter(|s| s.contour == contour && s.kind == SketchStepKind::Seg).count();
    let continues = info.steps[i + 1..].iter().any(|s| s.contour == contour);
    Some(Cur { start, end: st.point, tangent: st.tangent, segs, contour, continues })
}

/// The direct child of a block that contains `span`.
fn direct_child<'a>(blocks: &[(Span, Blk<'a>)], span: Span) -> Option<(usize, &'a Stmt)> {
    // innermost block first (holes are listed after their parents)
    for (bi, (_, b)) in blocks.iter().enumerate().rev() {
        if let Some(k) = b.children.iter().find(|k| k.span.start <= span.start && span.end <= k.span.end) {
            if call_name(k) == Some("hole") && k.span != span && blocks.iter().any(|(s, _)| *s == k.span) {
                continue;
            }
            return Some((bi, k));
        }
    }
    None
}

fn collect_blocks<'a>(text: &str, s: &'a Stmt, out: &mut Vec<(Span, Blk<'a>)>) {
    let Some(b) = block_of(text, s) else { return };
    let kids = b.children;
    out.push((s.span, b));
    for k in kids {
        if call_name(k) == Some("hole") {
            collect_blocks(text, k, out);
        }
    }
}

/// Sketch, insertion point and drawing state for the editor cursor (byte offset in the main
/// file). Without a sketch at the cursor, new statements go into a new `sketch { … }` block at
/// the end of the context module's body or of the file.
pub fn place<'o>(out: &'o EvalOutput, proj: &'o Project, cursor: u32) -> Place<'o> {
    let text = proj.main_text();
    let cursor = cursor.min(text.len() as u32);
    // the sketch containing the cursor, or starting on its line
    let (ls, le) = (line_start(text, cursor), line_end(text, cursor));
    let info = out
        .sketches
        .iter()
        .filter(|s| {
            s.span.file() == 0
                && ((s.span.start <= cursor && cursor <= s.span.end) || (ls <= s.span.start && s.span.start <= le))
        })
        .min_by_key(|s| s.span.len());
    let stmt = info.and_then(|i| find_stmt(&proj.main.stmts, i.span));
    let mut blocks = Vec::new();
    if let Some(s) = stmt {
        collect_blocks(text, s, &mut blocks);
    }
    let (info, frame) = match info {
        Some(i) if !blocks.is_empty() => (Some(i), sketch_frame(&out.tree, i.node)),
        _ => (None, DMat4::IDENTITY),
    };
    let (ins, pos) = match info {
        Some(_) => {
            // innermost block (hole) containing the cursor inside its braces
            let b = blocks
                .iter()
                .rev()
                .find(|(_, b)| line_end(text, b.open) < cursor && cursor <= b.close)
                .unwrap_or(&blocks[0]);
            ins_in(text, &b.1, cursor)
        }
        None => new_block(out, proj, text),
    };
    let cur = info.and_then(|i| cur_at(i, pos));
    let mut fresh = ins.clone();
    if let (Some(i), Some(c)) = (info, &cur)
        && c.continues
        && let Some(last) = info.and_then(|x| x.steps.iter().rposition(|s| s.contour == c.contour))
        && let Some((bi, k)) = direct_child(&blocks, i.steps[last].span)
    {
        fresh = after_stmt(text, k, &blocks[bi].1.indent);
    }
    let at = Span { start: pos, end: pos };
    let grids = out.grids.iter().filter(|g| g.in_context && span_visible(out, g.visible, at)).map(|g| g.id).collect();
    let insts = out
        .instances
        .iter()
        .filter(|i| i.label.is_some() && i.visible.is_some_and(|v| span_visible(out, v, at)))
        .map(|i| i.id)
        .collect();
    Place { info, frame, cur, pos, grids, insts, ins, fresh, blocks }
}

/// Insertion of a new sketch block: end of the context module's body, else of the file.
fn new_block(out: &EvalOutput, proj: &Project, text: &str) -> (Ins, u32) {
    if let Some(def) = out.context.def_span.filter(|s| s.file() == 0)
        && let Some(st) = find_stmt(&proj.main.stmts, def)
        && let StmtKind::Module(m) = &st.kind
        && m.body_span.file() == 0
        && text.as_bytes().get(m.body_span.start as usize) == Some(&b'{')
    {
        let close = m.body_span.end.saturating_sub(1);
        if text.as_bytes().get(close as usize) == Some(&b'}') {
            let base = indent_of(text, close);
            let indent = format!("{}    ", indent_of(text, st.span.start));
            let b = Blk { children: &m.body, open: m.body_span.start, close, indent: indent.clone(), base };
            let kids: Vec<&Stmt> = m.body.iter().filter(|c| c.span.file() == 0).collect();
            let mut ins = match kids.last() {
                Some(k) => after_stmt(text, k, &indent),
                None => into_empty(text, &b),
            };
            ins.block = Some(format!("{indent}    "));
            return (ins, close);
        }
    }
    let end = text.len() as u32;
    let before = if text.is_empty() {
        String::new()
    } else if text.ends_with("\n\n") {
        String::new()
    } else if text.ends_with('\n') {
        "\n".into()
    } else {
        "\n\n".into()
    };
    (Ins { at: end, before, sep: "\n".into(), after: "\n".into(), block: Some("    ".into()) }, end)
}

// ------------------------------------------------------------------------------------------
// snapping
// ------------------------------------------------------------------------------------------

/// Options of [`snap`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapOpts {
    /// Snap radius in sketch units.
    pub tol: f64,
    /// Rounding step of free positions (mm, 0 = none).
    pub step: f64,
    /// Alt: no snapping, only the step.
    pub free: bool,
    /// Previous point (horizontal/vertical).
    pub from: Option<DVec2>,
    /// Line from the contour's end along its tangent (`line(length = …)`).
    pub tangent: Option<(DVec2, DVec2)>,
    /// Only vertices (corner tool).
    pub vertices: bool,
}

struct Cand {
    p: DVec2,
    kind: SnapKind,
    text: Option<String>,
    grid: Option<GridSel>,
}

fn grid_text(name: &str, g: &GridSel) -> String {
    match &g.z {
        Some(z) => format!("{name}[{}, {}, {z}]", g.x, g.y),
        None => format!("{name}[{}, {}]", g.x, g.y),
    }
}

/// Grid points of the visible grids that lie in the sketch plane (values in the sketch's
/// coordinates, z = 0).
fn grid_points(out: &EvalOutput, place: &Place) -> Vec<Cand> {
    let mut res = Vec::new();
    for &gid in &place.grids {
        let Some(g) = out.grids.get(gid as usize) else { continue };
        let axis = |a: u8| g.planes.iter().filter(move |p| p.axis == a);
        for px in axis(0) {
            for py in axis(1) {
                for pz in axis(2) {
                    let v = g.frame_module.transform_point3(DVec3::new(px.value, py.value, pz.value));
                    if v.z.abs() > 1e-9 * (1.0 + v.length()) || res.len() > 20_000 {
                        continue;
                    }
                    let sel = GridSel {
                        grid: gid,
                        x: px.name.clone(),
                        y: py.name.clone(),
                        z: (!pz.implicit).then(|| pz.name.clone()),
                    };
                    res.push(Cand {
                        p: v.truncate(),
                        kind: SnapKind::Grid,
                        text: Some(grid_text(&g.name, &sel)),
                        grid: Some(sel),
                    });
                }
            }
        }
    }
    res
}

fn anchor_points(out: &EvalOutput, place: &Place) -> Vec<Cand> {
    let mut res = Vec::new();
    for &iid in &place.insts {
        let Some(inst) = out.instances.get(iid as usize) else { continue };
        let Some(label) = &inst.label else { continue };
        for a in &inst.anchors {
            let v = inst.matrix.transform_point3(a.point);
            if v.z.abs() > 1e-9 * (1.0 + v.length()) {
                continue;
            }
            res.push(Cand {
                p: v.truncate(),
                kind: SnapKind::Anchor,
                text: Some(format!("{label}.{}", a.name)),
                grid: None,
            });
        }
    }
    res
}

fn kseg(g: &Seg) -> Option<Segment> {
    g.to_kernel()
}

fn arc_mid(center: DVec2, r: f64, a0: f64, a1: f64) -> DVec2 {
    let m = (a0 + a1) / 2.0;
    center + DVec2::new(m.cos(), m.sin()) * r
}

/// Is angle `a` on the arc from `a0` to `a1`?
fn on_arc(a: f64, a0: f64, a1: f64) -> bool {
    let tau = std::f64::consts::TAU;
    let (lo, sw) = if a1 >= a0 { (a0, a1 - a0) } else { (a1, a0 - a1) };
    let d = (a - lo).rem_euclid(tau);
    d <= sw + 1e-9 || (sw >= tau - 1e-9)
}

/// Intersections of two segments (lines and arcs exact, curves by sampling).
fn intersections(a: &Segment, b: &Segment, size: f64) -> Vec<DVec2> {
    let eps = 1e-9 * (1.0 + size);
    let mut out = Vec::new();
    match (a, b) {
        (Segment::Line { a: p0, b: p1 }, Segment::Line { a: q0, b: q1 }) => {
            let r = *p1 - *p0;
            let s = *q1 - *q0;
            let den = r.perp_dot(s);
            if den.abs() > 1e-12 * r.length() * s.length() {
                let t = (*q0 - *p0).perp_dot(s) / den;
                let u = (*q0 - *p0).perp_dot(r) / den;
                if (-1e-9..=1.0 + 1e-9).contains(&t) && (-1e-9..=1.0 + 1e-9).contains(&u) {
                    out.push(*p0 + r * t);
                }
            }
        }
        (Segment::Line { a: p0, b: p1 }, Segment::Arc { center, r, a0, a1 })
        | (Segment::Arc { center, r, a0, a1 }, Segment::Line { a: p0, b: p1 }) => {
            let d = *p1 - *p0;
            let f = *p0 - *center;
            let (qa, qb, qc) = (d.dot(d), 2.0 * f.dot(d), f.dot(f) - r * r);
            let disc = qb * qb - 4.0 * qa * qc;
            if qa > 0.0 && disc >= -eps {
                let sq = disc.max(0.0).sqrt();
                for t in [(-qb - sq) / (2.0 * qa), (-qb + sq) / (2.0 * qa)] {
                    if (-1e-9..=1.0 + 1e-9).contains(&t) {
                        let p = *p0 + d * t;
                        let ang = (p - *center).y.atan2((p - *center).x);
                        if on_arc(ang, *a0, *a1) {
                            out.push(p);
                        }
                    }
                }
            }
        }
        (Segment::Arc { center: c0, r: r0, a0: s0, a1: e0 }, Segment::Arc { center: c1, r: r1, a0: s1, a1: e1 }) => {
            let dv = *c1 - *c0;
            let d = dv.length();
            if d > eps && d <= r0 + r1 + eps && d >= (r0 - r1).abs() - eps {
                let x = (d * d + r0 * r0 - r1 * r1) / (2.0 * d);
                let h = (r0 * r0 - x * x).max(0.0).sqrt();
                let m = *c0 + dv * (x / d);
                let n = DVec2::new(-dv.y, dv.x) / d;
                for p in [m + n * h, m - n * h] {
                    let ang0 = (p - *c0).y.atan2((p - *c0).x);
                    let ang1 = (p - *c1).y.atan2((p - *c1).x);
                    if on_arc(ang0, *s0, *e0) && on_arc(ang1, *s1, *e1) {
                        out.push(p);
                    }
                }
            }
        }
        _ => {
            let tol = (size * 1e-5).max(1e-7);
            let pa = a.sample(tol);
            let pb = b.sample(tol);
            for w in pa.windows(2) {
                for v in pb.windows(2) {
                    let la = Segment::Line { a: w[0], b: w[1] };
                    let lb = Segment::Line { a: v[0], b: v[1] };
                    out.extend(intersections(&la, &lb, size));
                }
            }
        }
    }
    out
}

/// Geometric segments of the sketch (steps with a segment) with their contour.
fn seg_steps(info: &SketchInfo) -> Vec<(usize, Segment)> {
    info.steps.iter().enumerate().filter_map(|(i, s)| s.seg.as_ref().and_then(kseg).map(|k| (i, k))).collect()
}

fn sketch_size(info: &SketchInfo) -> f64 {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for s in &info.steps {
        lo = lo.min(s.point);
        hi = hi.max(s.point);
    }
    if lo.x.is_finite() { (hi - lo).length().max(1.0) } else { 1.0 }
}

/// Vertices between two segments: (point, step index of the segment ending there or of the
/// `start` for the start vertex, incoming and outgoing segment).
pub fn vertices(info: &SketchInfo) -> Vec<(DVec2, usize, Segment, Segment)> {
    let mut res = Vec::new();
    let mut contours: Vec<u32> = info.steps.iter().map(|s| s.contour).collect();
    contours.dedup();
    for c in contours {
        let segs: Vec<(usize, Segment)> = info
            .steps
            .iter()
            .enumerate()
            .filter(|(_, s)| s.contour == c)
            .filter_map(|(i, s)| s.seg.as_ref().and_then(kseg).map(|k| (i, k)))
            .collect();
        for w in segs.windows(2) {
            res.push((w[0].1.end(), w[0].0, w[0].1.clone(), w[1].1.clone()));
        }
        let start = info.steps.iter().position(|s| s.contour == c && s.kind == SketchStepKind::Start);
        let closed = info.steps.iter().any(|s| s.contour == c && s.kind == SketchStepKind::Close)
            || segs
                .last()
                .zip(segs.first())
                .is_some_and(|(l, f)| l.1.end().distance(f.1.start()) <= 1e-9 * (1.0 + f.1.start().length()));
        if closed
            && segs.len() >= 2
            && let Some(si) = start
        {
            let (l, f) = (segs.last().unwrap(), segs.first().unwrap());
            res.push((f.1.start(), si, l.1.clone(), f.1.clone()));
        }
    }
    res
}

fn model_points(place: &Place) -> Vec<Cand> {
    let Some(info) = place.info else { return Vec::new() };
    let mut res = Vec::new();
    let size = sketch_size(info);
    let open_start = place.cur.as_ref().filter(|c| c.segs >= 2).map(|c| c.start);
    for s in &info.steps {
        if s.kind == SketchStepKind::Start || s.seg.is_some() {
            let kind = if open_start.is_some_and(|q| q.distance(s.point) <= 1e-9 * (1.0 + q.length())) {
                SnapKind::Start
            } else {
                SnapKind::End
            };
            res.push(Cand { p: s.point, kind, text: None, grid: None });
        }
    }
    let segs = seg_steps(info);
    for (_, k) in &segs {
        match k {
            Segment::Line { a, b } => {
                res.push(Cand { p: (*a + *b) / 2.0, kind: SnapKind::Mid, text: None, grid: None })
            }
            Segment::Arc { center, r, a0, a1 } => {
                res.push(Cand { p: *center, kind: SnapKind::Center, text: None, grid: None });
                res.push(Cand { p: arc_mid(*center, *r, *a0, *a1), kind: SnapKind::Mid, text: None, grid: None });
            }
            Segment::Nurbs(_) => {}
        }
    }
    let n = segs.len().min(200);
    for i in 0..n {
        for j in i + 1..n {
            for p in intersections(&segs[i].1, &segs[j].1, size) {
                // the common end of neighbours is an end point already
                let shared = [segs[i].1.start(), segs[i].1.end()].iter().any(|q| q.distance(p) <= 1e-7 * (1.0 + size));
                if !shared {
                    res.push(Cand { p, kind: SnapKind::Intersection, text: None, grid: None });
                }
            }
        }
    }
    res
}

/// Snaps a position in the sketch plane. Without Alt: the nearest point within `tol` (grid
/// points and anchors write their reference, other points numbers); else the tangent of the
/// contour, horizontal/vertical to the previous point and grid lines; else the step.
pub fn snap(out: &EvalOutput, place: &Place, q: DVec2, o: &SnapOpts) -> Snapped {
    if o.vertices {
        let best = place
            .info
            .map(vertices)
            .unwrap_or_default()
            .into_iter()
            .min_by(|a, b| a.0.distance(q).partial_cmp(&b.0.distance(q)).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((p, ..)) = best.filter(|v| v.0.distance(q) <= o.tol * 2.0) {
            return Snapped {
                pick: Pick::num(p, SnapKind::Vertex),
                label: SnapKind::Vertex.label().into(),
                guides: Vec::new(),
            };
        }
        let (p, text) = step_point(q, o.step);
        return Snapped {
            pick: Pick { p, text, kind: SnapKind::Step, grid: None },
            label: String::new(),
            guides: Vec::new(),
        };
    }
    if !o.free {
        let mut cands = grid_points(out, place);
        cands.extend(anchor_points(out, place));
        cands.extend(model_points(place));
        let best = cands.into_iter().filter(|c| c.p.distance(q) <= o.tol).min_by(|a, b| {
            let (da, db) = (a.p.distance(q), b.p.distance(q));
            // equal places: by kind (a grid point before an end point at the same spot)
            if (da - db).abs() <= 1e-6 * o.tol.max(1e-9) || a.p.distance(b.p) <= 1e-9 * (1.0 + a.p.length()) {
                a.kind.rank().cmp(&b.kind.rank())
            } else {
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            }
        });
        if let Some(c) = best {
            let text = c.text.unwrap_or_else(|| point_text(c.p));
            let label = if matches!(c.kind, SnapKind::Grid | SnapKind::Anchor) {
                text.clone()
            } else {
                format!("{} {}", c.kind.label(), point_text(c.p))
            };
            return Snapped { pick: Pick { p: c.p, text, kind: c.kind, grid: c.grid }, label, guides: Vec::new() };
        }
        if let Some((from, dir)) = o.tangent {
            let d = dir.normalize_or_zero();
            let t = (q - from).dot(d);
            let off = (q - from).perp_dot(d).abs();
            if t > o.tol && off <= o.tol {
                let len = if o.step > 0.0 { ((t / o.step).round() * o.step).max(o.step) } else { t };
                let p = from + d * len;
                let pick = Pick { p, text: point_text(p), kind: SnapKind::Tangent, grid: None };
                return Snapped {
                    pick,
                    label: format!("tangential, Länge {}", fmt_literal(len, o.step)),
                    guides: vec![vec![from, from + d * (len + o.tol * 4.0)]],
                };
            }
        }
        // one coordinate on a grid plane or on the previous point
        let mut xs: Vec<(f64, String, SnapKind)> = Vec::new();
        let mut ys: Vec<(f64, String, SnapKind)> = Vec::new();
        for &gid in &place.grids {
            let Some(g) = out.grids.get(gid as usize) else { continue };
            if g.frame_module != DMat4::IDENTITY {
                continue;
            }
            for p in g.planes.iter().filter(|p| !p.implicit) {
                let e = (p.value, format!("{}.{}", g.name, p.name), SnapKind::GridLine);
                match p.axis {
                    0 => xs.push(e),
                    1 => ys.push(e),
                    _ => {}
                }
            }
        }
        if let Some(f) = o.from {
            xs.push((f.x, num_text(f.x), SnapKind::Vertical));
            ys.push((f.y, num_text(f.y), SnapKind::Horizontal));
        }
        let pick_axis = |list: &[(f64, String, SnapKind)], v: f64| {
            list.iter()
                .filter(|e| (e.0 - v).abs() <= o.tol)
                .min_by(|a, b| {
                    let (da, db) = ((a.0 - v).abs(), (b.0 - v).abs());
                    // previous point before grid lines at the same place
                    if (da - db).abs() < 1e-12 {
                        (a.2 == SnapKind::GridLine).cmp(&(b.2 == SnapKind::GridLine))
                    } else {
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    }
                })
                .cloned()
        };
        let (bx, by) = (pick_axis(&xs, q.x), pick_axis(&ys, q.y));
        if bx.is_some() || by.is_some() {
            let (sq, _) = step_point(q, o.step);
            let tx = |v: f64| fmt_literal(v, o.step);
            let (x, xt) = bx.as_ref().map_or((sq.x, tx(sq.x)), |e| (e.0, e.1.clone()));
            let (y, yt) = by.as_ref().map_or((sq.y, tx(sq.y)), |e| (e.0, e.1.clone()));
            let p = DVec2::new(x, y);
            let kind = match (&bx, &by) {
                (_, Some(e)) if e.2 == SnapKind::Horizontal => SnapKind::Horizontal,
                (Some(e), _) if e.2 == SnapKind::Vertical => SnapKind::Vertical,
                _ => SnapKind::GridLine,
            };
            let mut guides = Vec::new();
            let span = o.tol * 30.0;
            if let Some(e) = &bx {
                let from_y = if e.2 == SnapKind::Vertical { o.from.map_or(y, |f| f.y) } else { y };
                guides.push(vec![DVec2::new(x, from_y.min(y) - span), DVec2::new(x, from_y.max(y) + span)]);
            }
            if let Some(e) = &by {
                let from_x = if e.2 == SnapKind::Horizontal { o.from.map_or(x, |f| f.x) } else { x };
                guides.push(vec![DVec2::new(from_x.min(x) - span, y), DVec2::new(from_x.max(x) + span, y)]);
            }
            let names: Vec<String> = [&bx, &by]
                .into_iter()
                .flatten()
                .map(|e| if e.2 == SnapKind::GridLine { e.1.clone() } else { e.2.label().to_string() })
                .collect();
            let text = format!("[{xt}, {yt}]");
            let label = format!("{} {}", names.join(", "), text);
            return Snapped { pick: Pick { p, text, kind, grid: None }, label, guides };
        }
    }
    let (p, text) = step_point(q, o.step);
    let label = text.clone();
    Snapped { pick: Pick { p, text, kind: SnapKind::Step, grid: None }, label, guides: Vec::new() }
}

/// Snap options for a tool at a stage: the previous point and the tangent line.
pub fn snap_opts(place: &Place, tool: SketchTool, picks: &[Pick], tol: f64, step: f64, free: bool) -> SnapOpts {
    let from = picks.last().map(|p| p.p).or_else(|| place.cur.as_ref().map(|c| c.end));
    let tangent = match (tool, picks.len(), &place.cur) {
        (SketchTool::Line, 1, Some(c)) if c.end.distance(picks[0].p) <= eps(c.end) => c.tangent.map(|t| (c.end, t)),
        _ => None,
    };
    SnapOpts { tol, step, free, from, tangent, vertices: tool == SketchTool::Corner && picks.is_empty() }
}

fn eps(p: DVec2) -> f64 {
    1e-6 * (1.0 + p.length())
}

// ------------------------------------------------------------------------------------------
// tools: prompts, previews, code
// ------------------------------------------------------------------------------------------

/// German prompt of a tool at a stage.
pub fn prompt(place: &Place, tool: SketchTool, picks: &[Pick]) -> String {
    let n = picks.len();
    let s = match tool {
        SketchTool::Select => "Auswahl: Segment anklicken (Entf löscht), Griffe ziehen",
        SketchTool::Line if n == 0 => "Linie: Anfangspunkt",
        SketchTool::Line => "Linie: nächster Punkt (Klick auf den Anfang schließt, Esc beendet)",
        SketchTool::ArcThrough if n == 0 => "Bogen (3 Punkte): Anfangspunkt",
        SketchTool::ArcThrough if n == 1 => "Bogen (3 Punkte): Punkt auf dem Bogen",
        SketchTool::ArcThrough => "Bogen (3 Punkte): Endpunkt",
        SketchTool::ArcCenter if n == 0 => "Bogen (Mittelpunkt): Anfangspunkt",
        SketchTool::ArcCenter if n == 1 => "Bogen (Mittelpunkt): Mittelpunkt",
        SketchTool::ArcCenter => "Bogen (Mittelpunkt): Endwinkel",
        SketchTool::ArcTangent if place.cur.as_ref().is_some_and(|c| c.tangent.is_some()) => {
            "Bogen (tangential): Endpunkt"
        }
        SketchTool::ArcTangent => "Bogen (tangential): braucht eine offene Kontur mit einem Segment",
        SketchTool::Bezier if n == 0 => "Bézier: Anfangspunkt",
        SketchTool::Bezier if n == 1 => "Bézier: Kontrollpunkt 1",
        SketchTool::Bezier if n == 2 => "Bézier: Kontrollpunkt 2",
        SketchTool::Bezier => "Bézier: Endpunkt",
        SketchTool::Rect if n == 0 => "Rechteck: erste Ecke",
        SketchTool::Rect => "Rechteck: gegenüberliegende Ecke",
        SketchTool::Circle if n == 0 => "Kreis: Mittelpunkt",
        SketchTool::Circle => "Kreis: Punkt auf dem Kreis",
        SketchTool::Corner if n == 0 => "Ecke runden: Ecke wählen",
        SketchTool::Corner => "Ecke runden: Radius (Abstand zur Ecke)",
        SketchTool::Close => "Schließen",
        SketchTool::Delete => "Segment löschen",
    };
    s.to_string()
}

fn sample(k: &Segment, size: f64) -> Vec<DVec2> {
    k.sample((size * 1e-3).clamp(1e-4, 0.05))
}

fn arc_through(p0: DVec2, q: DVec2, p1: DVec2) -> Option<Segment> {
    let (b, c) = (q - p0, p1 - p0);
    let den = 2.0 * b.perp_dot(c);
    if den.abs() <= 1e-12 * (b.length() * c.length()).max(1e-300) {
        return None;
    }
    let ux = (c.y * b.length_squared() - b.y * c.length_squared()) / den;
    let uy = (b.x * c.length_squared() - c.x * b.length_squared()) / den;
    let center = p0 + DVec2::new(ux, uy);
    let ccw = b.perp_dot(p1 - q) > 0.0;
    Some(arc_seg(center, p0, p1, ccw))
}

fn arc_seg(center: DVec2, from: DVec2, to: DVec2, ccw: bool) -> Segment {
    let tau = std::f64::consts::TAU;
    let a0 = (from - center).y.atan2((from - center).x);
    let a1 = (to - center).y.atan2((to - center).x);
    let mut d = (a1 - a0).rem_euclid(tau);
    if d < 1e-12 {
        d = tau;
    }
    let sw = if ccw { d } else { d - tau };
    Segment::Arc { center, r: (from - center).length(), a0, a1: a0 + sw }
}

fn tangent_arc(p0: DVec2, t: DVec2, p1: DVec2) -> Option<Segment> {
    let chord = p1 - p0;
    let d = chord.length();
    let t = t.normalize_or_zero();
    let n = DVec2::new(-t.y, t.x);
    let nd = n.dot(chord);
    if d <= 1e-12 || nd.abs() <= 1e-9 * d {
        return None;
    }
    let k = d * d / (2.0 * nd);
    Some(arc_seg(p0 + n * k, p0, p1, k > 0.0))
}

/// Signed sweep (degrees) from `p0` around `c` towards `q`, continuing `prev` (|sweep| may
/// exceed 180° when the mouse goes on around).
pub fn center_sweep(p0: DVec2, c: DVec2, q: DVec2, prev: Option<f64>) -> f64 {
    let a0 = (p0 - c).y.atan2((p0 - c).x);
    let a = (q - c).y.atan2((q - c).x);
    let base = (a - a0).to_degrees();
    let base = (base + 180.0).rem_euclid(360.0) - 180.0;
    match prev {
        Some(pv) => {
            // the candidate base + 360·k closest to the previous sweep
            let k = ((pv - base) / 360.0).round();
            let v = base + 360.0 * k;
            v.clamp(-360.0, 360.0)
        }
        None => base,
    }
}

fn round_deg(a: f64, step: f64) -> f64 {
    if step > 0.0 { (a / step).round() * step } else { a }
}

/// Polylines of the tool's result for the picked points and the cursor pick (sketch plane).
pub fn preview(
    place: &Place,
    tool: SketchTool,
    picks: &[Pick],
    at: &Pick,
    angle: Option<f64>,
    step: f64,
    step_deg: f64,
) -> Vec<Vec<DVec2>> {
    let size = place.info.map_or(50.0, sketch_size);
    let s = |k: Segment| sample(&k, size);
    let q = at.p;
    let mut out = Vec::new();
    match (tool, picks) {
        (SketchTool::Line, [p0, ..]) => out.push(vec![picks.last().unwrap_or(p0).p, q]),
        (SketchTool::ArcThrough, [p0]) | (SketchTool::ArcCenter, [p0]) | (SketchTool::Bezier, [p0]) => {
            out.push(vec![p0.p, q])
        }
        (SketchTool::ArcThrough, [p0, m, ..]) => match arc_through(p0.p, m.p, q) {
            Some(a) => out.push(s(a)),
            None => out.push(vec![p0.p, q]),
        },
        (SketchTool::ArcCenter, [p0, c, ..]) => {
            let sw = if at.kind.is_point() {
                center_sweep(p0.p, c.p, q, angle)
            } else {
                round_deg(center_sweep(p0.p, c.p, q, angle), step_deg)
            };
            if sw.abs() > 1e-9 {
                let r = p0.p.distance(c.p);
                let a0 = (p0.p - c.p).y.atan2((p0.p - c.p).x);
                out.push(s(Segment::Arc { center: c.p, r, a0, a1: a0 + sw.to_radians() }));
            }
            out.push(vec![c.p, p0.p]);
        }
        (SketchTool::ArcTangent, _) => {
            if let Some(c) = &place.cur
                && let Some(t) = c.tangent
                && let Some(a) = tangent_arc(c.end, t, q)
            {
                out.push(s(a));
            }
        }
        (SketchTool::Bezier, [p0, c1]) => out.push(vec![p0.p, c1.p, q]),
        (SketchTool::Bezier, [p0, c1, c2, ..]) => {
            out.push(s(Segment::bezier(&[p0.p, c1.p, c2.p, q])));
            out.push(vec![p0.p, c1.p, c2.p, q]);
        }
        (SketchTool::Rect, [a, ..]) => {
            let a = a.p;
            out.push(vec![a, DVec2::new(q.x, a.y), q, DVec2::new(a.x, q.y), a]);
        }
        (SketchTool::Circle, [c, ..]) => {
            let r = c.p.distance(q);
            if r > 1e-9 {
                let a0 = (q - c.p).y.atan2((q - c.p).x);
                out.push(s(Segment::Arc { center: c.p, r, a0, a1: a0 + std::f64::consts::TAU }));
                out.push(vec![c.p, q]);
            }
        }
        (SketchTool::Corner, [v, ..]) => {
            if let Some(info) = place.info
                && let Some((p, _, a, b)) = vertices(info).into_iter().find(|x| x.0.distance(v.p) <= eps(v.p))
            {
                let r = corner_radius(p, q, step);
                if let Ok(res) = round_corner(&a, &b, CornerKind::Round(r)) {
                    out.push(s(res.mid));
                }
                out.push(vec![p, q]);
            }
        }
        _ => {}
    }
    out
}

fn corner_radius(v: DVec2, q: DVec2, step: f64) -> f64 {
    let r = v.distance(q);
    if step > 0.0 { ((r / step).round() * step).max(step) } else { r }
}

/// Result of a click that completes a tool action.
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub edits: Vec<TextEdit>,
    /// Cursor after the edits: edit index and byte offset in its text.
    pub select: Option<(usize, usize)>,
    /// Points the tool keeps (a chain of lines continues at the last point).
    pub points: Vec<Pick>,
}

/// What a click does.
#[derive(Debug, Clone, PartialEq)]
pub enum Click {
    /// The point is kept; the action needs more.
    More(Vec<Pick>),
    Done(Action),
}

fn insert(place: &Place, stmts: &[String], fresh: bool) -> Action {
    let ins = if fresh { &place.fresh } else { &place.ins };
    let (e, end) = ins.edit(stmts);
    Action { edits: vec![e], select: Some((0, end)), points: Vec::new() }
}

/// Corner points of a rectangle: (bx, ay) and (ax, by) with grid references if both corners
/// are points of the same grid.
fn rect_corners(out: &EvalOutput, a: &Pick, b: &Pick) -> (String, String) {
    if let (Some(ga), Some(gb)) = (&a.grid, &b.grid)
        && ga.grid == gb.grid
        && ga.z == gb.z
        && let Some(g) = out.grids.get(ga.grid as usize)
    {
        let c1 = GridSel { grid: ga.grid, x: gb.x.clone(), y: ga.y.clone(), z: ga.z.clone() };
        let c2 = GridSel { grid: ga.grid, x: ga.x.clone(), y: gb.y.clone(), z: ga.z.clone() };
        return (grid_text(&g.name, &c1), grid_text(&g.name, &c2));
    }
    (point_text(DVec2::new(b.p.x, a.p.y)), point_text(DVec2::new(a.p.x, b.p.y)))
}

/// Handles a click of a drawing tool with the picked points (`picks` includes the new one).
#[allow(clippy::too_many_arguments)]
pub fn click(
    out: &EvalOutput,
    proj: &Project,
    place: &Place,
    tool: SketchTool,
    picks: &[Pick],
    step: f64,
    step_deg: f64,
    angle: Option<f64>,
) -> Result<Click, String> {
    let n = picks.len();
    let need_start = |p0: &Pick| place.cur.as_ref().is_none_or(|c| c.end.distance(p0.p) > eps(p0.p));
    let start = |p0: &Pick| format!("start({});", p0.text);
    let same = |a: &Pick, b: &Pick| a.p.distance(b.p) <= eps(a.p);
    match tool {
        SketchTool::Select | SketchTool::Close | SketchTool::Delete => Err("kein Zeichenwerkzeug".into()),
        SketchTool::Line => {
            if n < 2 {
                return Ok(Click::More(picks.to_vec()));
            }
            let (p0, p1) = (&picks[n - 2], &picks[n - 1]);
            if same(p0, p1) {
                return Ok(Click::More(picks[..n - 1].to_vec()));
            }
            let ns = need_start(p0);
            if !ns
                && p1.kind == SnapKind::Start
                && place.cur.as_ref().is_some_and(|c| c.segs >= 2 && c.start.distance(p1.p) <= eps(p1.p))
            {
                return Ok(Click::Done(insert(place, &["close();".into()], false)));
            }
            let mut stmts = Vec::new();
            if ns {
                stmts.push(start(p0));
            }
            if p1.kind == SnapKind::Tangent && !ns {
                let len = p0.p.distance(p1.p);
                stmts.push(format!("line(length = {});", fmt_literal(len, step)));
            } else {
                stmts.push(format!("line(to = {});", p1.text));
            }
            let mut a = insert(place, &stmts, ns && place.cur.as_ref().is_some_and(|c| c.continues));
            a.points = vec![p1.clone()];
            Ok(Click::Done(a))
        }
        SketchTool::ArcThrough => {
            if n < 3 {
                return Ok(Click::More(picks.to_vec()));
            }
            let (p0, m, p1) = (&picks[n - 3], &picks[n - 2], &picks[n - 1]);
            if arc_through(p0.p, m.p, p1.p).is_none() || same(p0, p1) {
                return Err("die drei Punkte liegen auf einer Geraden".into());
            }
            let ns = need_start(p0);
            let mut stmts = Vec::new();
            if ns {
                stmts.push(start(p0));
            }
            stmts.push(format!("arc(through = {}, to = {});", m.text, p1.text));
            let mut a = insert(place, &stmts, ns && place.cur.as_ref().is_some_and(|c| c.continues));
            a.points = vec![p1.clone()];
            Ok(Click::Done(a))
        }
        SketchTool::ArcCenter => {
            if n < 3 {
                if n == 2 && same(&picks[0], &picks[1]) {
                    return Err("Mittelpunkt liegt auf dem Anfangspunkt".into());
                }
                return Ok(Click::More(picks.to_vec()));
            }
            let (p0, c, p1) = (&picks[n - 3], &picks[n - 2], &picks[n - 1]);
            let sw = center_sweep(p0.p, c.p, p1.p, angle);
            let ns = need_start(p0);
            let mut stmts = Vec::new();
            if ns {
                stmts.push(start(p0));
            }
            let end;
            if p1.kind.is_point() && !same(c, p1) {
                if sw.abs() < 1e-9 {
                    return Err("Endpunkt gleich Anfangspunkt".into());
                }
                let cw = if sw < 0.0 { ", cw = true" } else { "" };
                stmts.push(format!("arc(center = {}, to = {}{cw});", c.text, p1.text));
                let r = p0.p.distance(c.p);
                end = c.p + (p1.p - c.p).normalize_or_zero() * r;
            } else {
                let a = round_deg(sw, step_deg);
                if a.abs() < 1e-9 {
                    return Err("Winkel 0°".into());
                }
                stmts.push(format!("arc(center = {}, angle = {});", c.text, fmt_num(a)));
                let r = p0.p.distance(c.p);
                let a0 = (p0.p - c.p).y.atan2((p0.p - c.p).x) + a.to_radians();
                end = c.p + DVec2::new(a0.cos(), a0.sin()) * r;
            }
            let mut a = insert(place, &stmts, ns && place.cur.as_ref().is_some_and(|c| c.continues));
            a.points = vec![Pick::num(end, SnapKind::End)];
            Ok(Click::Done(a))
        }
        SketchTool::ArcTangent => {
            let Some(c) = place.cur.as_ref().filter(|c| c.tangent.is_some()) else {
                return Err("tangentialer Bogen braucht eine offene Kontur mit einem Segment".into());
            };
            let p1 = &picks[n - 1];
            if tangent_arc(c.end, c.tangent.unwrap_or(DVec2::X), p1.p).is_none() {
                return Err("Endpunkt liegt auf der Tangente (Linie verwenden)".into());
            }
            let a = insert(place, &[format!("arc(to = {});", p1.text)], false);
            Ok(Click::Done(a))
        }
        SketchTool::Bezier => {
            if n < 4 {
                return Ok(Click::More(picks.to_vec()));
            }
            let (p0, c1, c2, p1) = (&picks[n - 4], &picks[n - 3], &picks[n - 2], &picks[n - 1]);
            let ns = need_start(p0);
            let mut stmts = Vec::new();
            if ns {
                stmts.push(start(p0));
            }
            stmts.push(format!("bezier(c1 = {}, c2 = {}, to = {});", c1.text, c2.text, p1.text));
            let mut a = insert(place, &stmts, ns && place.cur.as_ref().is_some_and(|c| c.continues));
            a.points = vec![p1.clone()];
            Ok(Click::Done(a))
        }
        SketchTool::Rect => {
            if n < 2 {
                return Ok(Click::More(picks.to_vec()));
            }
            let (a, b) = (&picks[n - 2], &picks[n - 1]);
            if (a.p.x - b.p.x).abs() <= eps(a.p) || (a.p.y - b.p.y).abs() <= eps(a.p) {
                return Err("Rechteck ohne Fläche".into());
            }
            let (c1, c2) = rect_corners(out, a, b);
            let stmts = vec![
                start(a),
                format!("line(to = {c1});"),
                format!("line(to = {});", b.text),
                format!("line(to = {c2});"),
                "close();".to_string(),
            ];
            Ok(Click::Done(insert(place, &stmts, place.cur.as_ref().is_some_and(|c| c.continues))))
        }
        SketchTool::Circle => {
            if n < 2 {
                return Ok(Click::More(picks.to_vec()));
            }
            let (c, r) = (&picks[n - 2], &picks[n - 1]);
            if same(c, r) {
                return Ok(Click::More(picks[..n - 1].to_vec()));
            }
            let stmts = vec![start(r), format!("arc(center = {}, angle = 360);", c.text), "close();".to_string()];
            Ok(Click::Done(insert(place, &stmts, place.cur.as_ref().is_some_and(|c| c.continues))))
        }
        SketchTool::Corner => {
            if n < 2 {
                if picks[0].kind != SnapKind::Vertex {
                    return Ok(Click::More(Vec::new()));
                }
                return Ok(Click::More(picks.to_vec()));
            }
            corner_edit(proj, place, &picks[0], &picks[1], step).map(Click::Done)
        }
    }
}

/// `corner(r = …)` at a vertex: after the segment ending there (or after `start` for the start
/// vertex); an existing `corner()` there gets the new radius.
fn corner_edit(proj: &Project, place: &Place, v: &Pick, at: &Pick, step: f64) -> Result<Action, String> {
    let info = place.info.ok_or("keine Skizze")?;
    let text = proj.main_text();
    let (p, si, ..) =
        vertices(info).into_iter().find(|x| x.0.distance(v.p) <= eps(v.p)).ok_or("keine Ecke an diesem Punkt")?;
    let r = corner_radius(p, at.p, step);
    let rt = fmt_literal(r, step);
    // an existing corner() right after the segment
    if let Some(next) = info.steps.get(si + 1)
        && next.kind == SketchStepKind::Corner
        && next.contour == info.steps[si].contour
        && let Some(st) = find_stmt(&proj.main.stmts, next.span)
        && let StmtKind::Call { args, .. } = &st.kind
        && let Some(arg) = args.iter().find(|a| a.name.as_ref().is_none_or(|n| n.name == "r"))
    {
        let e = TextEdit { span: arg.value.span, text: rt.clone() };
        return Ok(Action { edits: vec![e], select: Some((0, rt.len())), points: Vec::new() });
    }
    let (bi, k) =
        direct_child(&place.blocks, info.steps[si].span).filter(|(_, k)| k.span == info.steps[si].span).ok_or(
            "die Ecke gehört zu einer Anweisung in einer Schleife oder einem Modul: corner() dort von Hand einfügen",
        )?;
    let ins = after_stmt(text, k, &place.blocks[bi].1.indent);
    let (e, end) = ins.edit(&[format!("corner(r = {rt});")]);
    Ok(Action { edits: vec![e], select: Some((0, end)), points: Vec::new() })
}

/// `close();` for the open contour at the insertion point.
pub fn close(place: &Place) -> Result<Action, String> {
    match &place.cur {
        Some(c) if c.segs >= 1 => Ok(insert(place, &["close();".into()], false)),
        Some(_) => Err("Kontur ohne Segmente".into()),
        None => Err("keine offene Kontur an dieser Stelle".into()),
    }
}

/// Edit removing a statement: its whole line if it stands alone there.
fn remove_stmt(text: &str, span: Span) -> Span {
    let ls = line_start(text, span.start);
    let le = line_end(text, span.end);
    let before = &text[ls as usize..span.start as usize];
    let after = text[span.end as usize..le as usize].trim();
    if before.trim().is_empty() && (after.is_empty() || after.starts_with("//")) {
        let end = if (le as usize) < text.len() { le + 1 } else { le };
        let start = if end == le && ls > 0 { ls - 1 } else { ls };
        Span { start, end }
    } else {
        let mut end = span.end as usize;
        while end < le as usize && text.as_bytes()[end] == b' ' {
            end += 1;
        }
        Span { start: span.start, end: end as u32 }
    }
}

/// Removes the segment statement at the cursor (statement containing it or starting on its
/// line). The only segment of a contour takes the contour with it (`start`, `close`,
/// `corner`); a `corner()` right after the segment goes as well.
pub fn delete_segment(proj: &Project, place: &Place, cursor: u32) -> Result<Vec<TextEdit>, String> {
    let info = place.info.ok_or("keine Skizze am Cursor")?;
    let text = proj.main_text();
    let (ls, le) = (line_start(text, cursor.min(text.len() as u32)), line_end(text, cursor.min(text.len() as u32)));
    let is_seg =
        |s: &SketchStep| matches!(s.kind, SketchStepKind::Seg | SketchStepKind::Close | SketchStepKind::Corner);
    let i = info
        .steps
        .iter()
        .position(|s| is_seg(s) && inside(s, info.span) && s.span.start <= cursor && cursor <= s.span.end)
        .or_else(|| {
            info.steps
                .iter()
                .position(|s| is_seg(s) && inside(s, info.span) && ls <= s.span.start && s.span.start <= le)
        })
        .ok_or("kein Segment am Cursor")?;
    let st = &info.steps[i];
    let mut spans = vec![st.span];
    let geo = |s: &SketchStep| s.seg.is_some();
    let contour_geo = info.steps.iter().filter(|s| s.contour == st.contour && geo(s)).count();
    if geo(st) && contour_geo <= 1 {
        spans = info.steps.iter().filter(|s| s.contour == st.contour).map(|s| s.span).collect();
    } else if let Some(next) = info.steps.get(i + 1)
        && next.kind == SketchStepKind::Corner
        && next.contour == st.contour
        && st.kind == SketchStepKind::Seg
    {
        spans.push(next.span);
    }
    let mut cut: Vec<Span> = Vec::new();
    for sp in spans {
        let Some((_, k)) = direct_child(&place.blocks, sp).filter(|(_, k)| k.span == sp) else {
            if sp == st.span {
                return Err("das Segment steht in einer Schleife oder einem Modul: dort von Hand löschen".into());
            }
            continue;
        };
        let r = remove_stmt(text, k.span);
        if !cut.contains(&r) {
            cut.push(r);
        }
    }
    cut.sort_by_key(|s| s.start);
    let mut merged: Vec<Span> = Vec::new();
    for s in cut {
        match merged.last_mut() {
            Some(m) if s.start <= m.end => m.end = m.end.max(s.end),
            _ => merged.push(s),
        }
    }
    Ok(merged.into_iter().map(|span| TextEdit { span, text: String::new() }).collect())
}

/// Segment statement whose drawn segment is nearest to `q` within `tol` (select tool).
pub fn segment_at(out: &EvalOutput, place: &Place, q: DVec2, tol: f64) -> Option<Span> {
    let info = place.info?;
    let Op::Profile(Profile::Sketch { contours, .. }) = &out.tree.node(info.node).op else { return None };
    let size = sketch_size(info);
    let mut best: Option<(f64, Span)> = None;
    for (c, ci) in contours.iter().zip(&info.contours) {
        for (g, gi) in c.segs.iter().zip(&ci.segs) {
            if gi.implicit || gi.span.file() != 0 {
                continue;
            }
            let Some(k) = kseg(g) else { continue };
            let pts = sample(&k, size);
            for w in pts.windows(2) {
                let d = w[1] - w[0];
                let t = if d.length_squared() > 0.0 {
                    ((q - w[0]).dot(d) / d.length_squared()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let dist = (w[0] + d * t).distance(q);
                if dist <= tol && best.is_none_or(|b| dist < b.0) {
                    best = Some((dist, gi.span));
                }
            }
        }
    }
    best.map(|b| b.1)
}

/// Drawing raster in the sketch plane: lines of the visible grids (axis-aligned ones), grid
/// points, and the extent (sketch, grids, origin) for the step raster of the UI.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Raster {
    pub lines: Vec<[DVec2; 2]>,
    pub points: Vec<DVec2>,
    pub min: DVec2,
    pub max: DVec2,
}

pub fn raster(out: &EvalOutput, place: &Place) -> Raster {
    let pts = grid_points(out, place);
    let mut lo = DVec2::ZERO;
    let mut hi = DVec2::ZERO;
    for p in pts.iter().map(|c| c.p).chain(place.info.into_iter().flat_map(|i| i.steps.iter().map(|s| s.point))) {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let pad = ((hi - lo).max_element() * 0.15).max(10.0);
    let (lo, hi) = (lo - DVec2::splat(pad), hi + DVec2::splat(pad));
    let mut lines = Vec::new();
    for &gid in &place.grids {
        let Some(g) = out.grids.get(gid as usize) else { continue };
        if g.frame_module != DMat4::IDENTITY {
            continue;
        }
        for p in g.planes.iter().filter(|p| !p.implicit).take(400) {
            match p.axis {
                0 => lines.push([DVec2::new(p.value, lo.y), DVec2::new(p.value, hi.y)]),
                1 => lines.push([DVec2::new(lo.x, p.value), DVec2::new(hi.x, p.value)]),
                _ => {}
            }
        }
    }
    Raster { lines, points: pts.into_iter().map(|c| c.p).collect(), min: lo, max: hi }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_continues_beyond_half_turn() {
        let c = DVec2::ZERO;
        let p0 = DVec2::new(10.0, 0.0);
        // 170° then 190° (mouse went on around)
        let a = center_sweep(p0, c, DVec2::new((170f64).to_radians().cos(), (170f64).to_radians().sin()), None);
        assert!((a - 170.0).abs() < 1e-9);
        let q = DVec2::new((190f64).to_radians().cos(), (190f64).to_radians().sin());
        assert!((center_sweep(p0, c, q, Some(a)) - 190.0).abs() < 1e-9);
        assert!((center_sweep(p0, c, q, None) + 170.0).abs() < 1e-9);
    }

    #[test]
    fn intersections_of_lines_and_arcs() {
        let l1 = Segment::Line { a: DVec2::new(-10.0, 0.0), b: DVec2::new(10.0, 0.0) };
        let l2 = Segment::Line { a: DVec2::new(0.0, -10.0), b: DVec2::new(0.0, 10.0) };
        assert_eq!(intersections(&l1, &l2, 20.0), vec![DVec2::ZERO]);
        let arc = Segment::Arc { center: DVec2::ZERO, r: 5.0, a0: 0.0, a1: std::f64::consts::PI };
        let hits = intersections(&l2, &arc, 20.0);
        assert_eq!(hits.len(), 1);
        assert!((hits[0] - DVec2::new(0.0, 5.0)).length() < 1e-9);
        let arc2 = Segment::Arc { center: DVec2::new(5.0, 0.0), r: 5.0, a0: 0.0, a1: std::f64::consts::TAU };
        let full = Segment::Arc { center: DVec2::ZERO, r: 5.0, a0: 0.0, a1: std::f64::consts::TAU };
        assert_eq!(intersections(&full, &arc2, 20.0).len(), 2);
    }

    #[test]
    fn step_rounding_writes_short_numbers() {
        let (p, t) = step_point(DVec2::new(12.34, -0.2), 1.0);
        assert_eq!(p, DVec2::new(12.0, 0.0));
        assert_eq!(t, "[12, 0]");
        let (_, t) = step_point(DVec2::new(1.26, 2.0), 0.5);
        assert_eq!(t, "[1.5, 2]");
    }
}
