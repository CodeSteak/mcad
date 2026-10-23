//! Writing UI changes back into the source (Spec: Griffe und Rückschreiben).

use std::collections::BTreeMap;

use cad_syntax::Span;
use cad_syntax::ast::File;
use glam::DVec3;
use serde::{Deserialize, Serialize};

use crate::output::{EvalOutput, Handle, HandleKind, TextEdit};
use crate::value::{GridId, InstId, fmt_literal};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DragValue {
    /// New position in the handle's local system.
    Point(DVec3),
    /// New value of a 1D handle (radius, height, angle, plane, scale).
    Scalar(f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SnapRef {
    /// Grid point by plane names per axis (`_` for an implicit plane).
    Grid { grid: GridId, planes: [String; 3] },
    /// Anchor `name` of the named instance `inst`.
    Anchor { inst: InstId, name: String },
    /// Grid plane `g.name` as a number (1D handles: height, radius, plane).
    Plane { grid: GridId, plane: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DragTarget {
    /// Free drag (Alt): numbers rounded to the step are written into the literals.
    Free(DragValue),
    /// Drop on a grid point or anchor: a reference is written.
    Snap(SnapRef),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DragOptions {
    /// Rounding step for lengths in mm (0 = no rounding).
    pub step_mm: f64,
    /// Rounding step for angles in degrees.
    pub step_deg: f64,
    /// Rounding step for scale factors.
    pub step_scale: f64,
}

impl Default for DragOptions {
    fn default() -> Self {
        DragOptions { step_mm: 1.0, step_deg: 15.0, step_scale: 0.1 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapTarget {
    /// Text written into the source, e.g. `g[rechts, hinten, deckel]` or `k.ecke`.
    pub text: String,
    pub r#ref: SnapRef,
    /// Value the reference evaluates to (numbers in the handle's local system).
    pub local: DVec3,
    /// Where the referenced point is (context root coordinates).
    pub world: DVec3,
    /// Where the handle ends up after snapping (`handle.frame * local`). Equal to
    /// `world` when the handle's statement is not transformed within its module.
    pub result_world: DVec3,
}

fn step_for(h: &Handle, o: &DragOptions) -> f64 {
    match h.kind {
        HandleKind::Angle { .. } => o.step_deg,
        HandleKind::Scale { .. } => o.step_scale,
        _ => o.step_mm,
    }
}

fn in_src(src: &str, s: Span) -> bool {
    (s.end as usize) <= src.len()
        && s.start <= s.end
        && src.is_char_boundary(s.start as usize)
        && src.is_char_boundary(s.end as usize)
}

/// Text edits for dragging handle `handle_id` to `target`. Empty if the handle does
/// not exist or the target does not apply.
pub fn drag_edits(
    out: &EvalOutput,
    src: &str,
    handle_id: u64,
    target: &DragTarget,
    opts: &DragOptions,
) -> Vec<TextEdit> {
    let Some(h) = out.handle(handle_id) else {
        return Vec::new();
    };
    let edits = match target {
        DragTarget::Free(v) => free_edits(h, *v, opts),
        DragTarget::Snap(r) => snap_edit(out, h, r).into_iter().collect(),
    };
    edits
        .into_iter()
        .filter(|e| in_src(src, e.span) && src[e.span.start as usize..e.span.end as usize] != e.text)
        .collect()
}

fn free_edits(h: &Handle, v: DragValue, opts: &DragOptions) -> Vec<TextEdit> {
    let n = h.value.len();
    let vals: Vec<f64> = match (v, n) {
        (DragValue::Scalar(x), 1) => vec![x],
        (DragValue::Point(p), 1) => {
            // Project onto the handle's axis.
            let (Some(axis), Some(base)) = (h.axis, h.base) else { return Vec::new() };
            if h.kind.is_angle() {
                return Vec::new();
            }
            let a = axis.normalize_or_zero();
            vec![(p - base).dot(a)]
        }
        (DragValue::Point(p), _) => (0..n).map(|i| p[i]).collect(),
        (DragValue::Scalar(_), _) => return Vec::new(),
    };
    let step = step_for(h, opts);
    if h.comps.iter().any(|c| c.is_some()) {
        // (new values, current values) per literal.
        let mut by_span: BTreeMap<(u32, u32), (Vec<f64>, Vec<f64>)> = BTreeMap::new();
        for (i, c) in h.comps.iter().enumerate() {
            if let (Some(c), Some(v)) = (c, vals.get(i)) {
                let e = by_span.entry((c.span.start, c.span.end)).or_default();
                e.0.push(v * c.scale);
                e.1.push(h.value.get(i).copied().unwrap_or(f64::NAN) * c.scale);
            }
        }
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        return by_span
            .into_iter()
            // A literal whose value does not change keeps its text (no re-rounding of the
            // components an axis-constrained drag leaves alone).
            .filter(|(_, (new, cur))| !((mean(new) - mean(cur)).abs() <= 1e-9 * mean(cur).abs().max(1.0)))
            .map(|((start, end), (vs, _))| TextEdit { span: Span { start, end }, text: fmt_literal(mean(&vs), step) })
            .collect();
    }
    if h.has_ref {
        // A grid / anchor reference without literals: replace it by numbers.
        let span = h.def_span.unwrap_or(h.expr_span);
        let text = if n == 1 {
            fmt_literal(vals[0], step)
        } else {
            format!("[{}]", vals.iter().map(|v| fmt_literal(*v, step)).collect::<Vec<_>>().join(", "))
        };
        return vec![TextEdit { span, text }];
    }
    Vec::new()
}

/// Text of a snap reference, if it is valid.
fn ref_text(out: &EvalOutput, r: &SnapRef) -> Option<String> {
    match r {
        SnapRef::Grid { grid, planes } => {
            let g = out.grids.get(*grid as usize)?;
            for (axis, name) in planes.iter().enumerate() {
                if !g.planes.iter().any(|p| p.axis as usize == axis && p.name == *name) {
                    return None;
                }
            }
            Some(format!("{}[{}]", g.name, planes.join(", ")))
        }
        SnapRef::Anchor { inst, name } => {
            let i = out.instances.get(*inst as usize)?;
            let label = i.label.as_ref()?;
            i.anchors.iter().any(|a| a.name == *name).then(|| format!("{label}.{name}"))
        }
        SnapRef::Plane { grid, plane } => {
            let g = out.grids.get(*grid as usize)?;
            g.planes.iter().any(|p| p.name == *plane && !p.implicit).then(|| format!("{}.{plane}", g.name))
        }
    }
}

/// Whether the reference can be written at `span` (inside the visibility range and
/// not inside a module definition nested in that range).
fn visible_at(out: &EvalOutput, r: &SnapRef, span: Span) -> bool {
    let vis = match r {
        SnapRef::Grid { grid, .. } | SnapRef::Plane { grid, .. } => out.grids.get(*grid as usize).map(|g| g.visible),
        SnapRef::Anchor { inst, .. } => out.instances.get(*inst as usize).and_then(|i| i.visible),
    };
    let Some(vis) = vis else { return false };
    if !(vis.start <= span.start && span.end <= vis.end) {
        return false;
    }
    !out.modules.iter().any(|m| vis.start <= m.span.start && m.span.start <= span.start && span.end <= m.span.end)
}

/// 1D handles that can snap onto grid planes (`g.name`).
fn is_linear(k: HandleKind) -> bool {
    matches!(k, HandleKind::Radius | HandleKind::Height | HandleKind::Plane { .. })
}

fn snap_edit(out: &EvalOutput, h: &Handle, r: &SnapRef) -> Option<TextEdit> {
    match r {
        SnapRef::Plane { .. } => {
            // Only planes the handle actually lands on (see `plane_targets`).
            if !plane_targets(out, h).iter().any(|t| t.r#ref == *r) {
                return None;
            }
        }
        _ => {
            if h.kind != HandleKind::Point || h.comps.iter().flatten().any(|c| c.scale != 1.0) {
                return None;
            }
        }
    }
    let text = ref_text(out, r)?;
    for span in [h.def_span, Some(h.expr_span)].into_iter().flatten() {
        if visible_at(out, r, span) {
            return Some(TextEdit { span, text });
        }
    }
    None
}

/// Grid planes a 1D handle (height, radius, plane) can snap to: planes of the visible grids
/// perpendicular to the handle's axis whose value, written as `g.name` into the handle's
/// literal, puts the handle exactly onto the plane (same origin and unit along the axis).
fn plane_targets(out: &EvalOutput, h: &Handle) -> Vec<SnapTarget> {
    let mut res = Vec::new();
    if !is_linear(h.kind) || h.comps.len() != 1 {
        return res;
    }
    let (Some(axis), Some(base)) = (h.axis, h.base) else { return res };
    // Literal = value * scale; without a literal (pure reference) scale 1.
    let scale = h.comps[0].map_or(1.0, |c| c.scale);
    if scale == 0.0 {
        return res;
    }
    let a_w = h.frame.transform_vector3(axis);
    let b_w = h.frame.transform_point3(base);
    if a_w.length_squared() < 1e-24 {
        return res;
    }
    for &gid in &h.grids {
        let Some(g) = out.grids.get(gid as usize) else { continue };
        for p in &g.planes {
            if p.implicit {
                continue;
            }
            let mut e = DVec3::ZERO;
            e[p.axis as usize] = 1.0;
            let n_w = g.frame.transform_vector3(e).normalize_or_zero();
            let q_w = g.frame.transform_point3(e * p.value);
            let an = a_w.dot(n_w);
            // Axis perpendicular to the plane.
            if an.abs() < (1.0 - 1e-9) * a_w.length() {
                continue;
            }
            let t = (q_w - b_w).dot(n_w) / an;
            let written = p.value / scale;
            if (t - written).abs() > 1e-6 * t.abs().max(1.0) {
                continue;
            }
            let r = SnapRef::Plane { grid: gid, plane: p.name.clone() };
            let Some(text) = ref_text(out, &r) else { continue };
            let local = base + axis * written;
            let world = h.frame.transform_point3(local);
            res.push(SnapTarget { text, r#ref: r, local, world, result_world: world });
        }
    }
    res
}

/// Grid points and anchors a point handle can snap to; grid planes (`g.name`) for 1D
/// handles (height, radius, plane).
pub fn snap_targets(out: &EvalOutput, handle_id: u64) -> Vec<SnapTarget> {
    let Some(h) = out.handle(handle_id) else {
        return Vec::new();
    };
    if is_linear(h.kind) {
        return plane_targets(out, h)
            .into_iter()
            .filter(|t| [h.def_span, Some(h.expr_span)].into_iter().flatten().any(|s| visible_at(out, &t.r#ref, s)))
            .collect();
    }
    if h.kind != HandleKind::Point || h.comps.iter().flatten().any(|c| c.scale != 1.0) {
        return Vec::new();
    }
    let mut res = Vec::new();
    for &gid in &h.grids {
        let Some(g) = out.grids.get(gid as usize) else { continue };
        let inst_world = out.instances.get(g.module_inst as usize).map_or(glam::DMat4::IDENTITY, |i| i.world);
        let per_axis: Vec<Vec<&crate::output::PlaneInfo>> =
            (0..3u8).map(|a| g.planes.iter().filter(|p| p.axis == a).collect()).collect();
        for px in &per_axis[0] {
            for py in &per_axis[1] {
                for pz in &per_axis[2] {
                    let local = g.frame_module.transform_point3(DVec3::new(px.value, py.value, pz.value));
                    let planes = [px.name.clone(), py.name.clone(), pz.name.clone()];
                    let r = SnapRef::Grid { grid: gid, planes };
                    let Some(text) = ref_text(out, &r) else { continue };
                    res.push(SnapTarget {
                        text,
                        r#ref: r,
                        local,
                        world: inst_world.transform_point3(local),
                        result_world: h.frame.transform_point3(local),
                    });
                }
            }
        }
    }
    for &iid in &h.instances {
        let Some(inst) = out.instances.get(iid as usize) else { continue };
        let parent_world =
            inst.parent.and_then(|p| out.instances.get(p as usize)).map_or(glam::DMat4::IDENTITY, |p| p.world);
        for a in &inst.anchors {
            let local = inst.matrix.transform_point3(a.point);
            let r = SnapRef::Anchor { inst: iid, name: a.name.clone() };
            let Some(text) = ref_text(out, &r) else { continue };
            res.push(SnapTarget {
                text,
                r#ref: r,
                local,
                world: parent_world.transform_point3(local),
                result_world: h.frame.transform_point3(local),
            });
        }
    }
    res
}

/// Edit that writes `value_text` as the new default of parameter `param` of module
/// `module` ("Als Standard übernehmen"). `None` if module or parameter don't exist.
pub fn set_default_edit(src: &str, file: &File, module: &str, param: &str, value_text: &str) -> Option<TextEdit> {
    let path = crate::eval::find_module_path(&file.stmts, module)?;
    let m = path.last()?;
    let p = m.params.iter().find(|p| p.name.name == param)?;
    let edit = match &p.default {
        Some(d) => TextEdit { span: d.span, text: value_text.to_string() },
        None => {
            TextEdit { span: Span { start: p.name.span.end, end: p.name.span.end }, text: format!(" = {value_text}") }
        }
    };
    in_src(src, edit.span).then_some(edit)
}
