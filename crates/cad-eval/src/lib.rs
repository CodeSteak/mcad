//! Evaluation of the CAD language: syntax tree → geometry tree (`cad_ir::Tree`) plus
//! everything the UI needs without the kernel: inline hints, grids, module instances
//! with anchors, handles for editing literals, echo output and module signatures.
//!
//! Entry point: [`eval`]. Writing UI changes back into the source: [`drag_edits`],
//! [`snap_targets`], [`set_default_edit`].
//!
//! The evaluation never calls a kernel. It is deterministic: the same source and
//! options give the same output (including `rands()` and all ids).

mod builtin_fn;
mod builtin_mod;
mod colors;
mod edit;
pub mod env;
mod eval;
mod expr;
mod grid;
mod handles;
pub mod math;
mod ops;
mod output;
mod print;
mod sig;
pub mod value;

pub use edit::{DragOptions, DragTarget, DragValue, SnapRef, SnapTarget, drag_edits, set_default_edit, snap_targets};
pub use output::*;
pub use value::{Origin, PValue, fmt_literal, fmt_num};

/// Evaluates a parsed file (see [`eval::eval`]). Handles of computed values are moved from
/// `handles` to `locked_handles`: they are shown, but not edited (Spec: Griffe, Regel 3).
pub fn eval(src: &str, file: &cad_syntax::ast::File, opts: &EvalOptions) -> EvalOutput {
    let mut out = eval::eval(src, file, opts);
    let (locked, handles): (Vec<Handle>, Vec<Handle>) = std::mem::take(&mut out.handles).into_iter().partition(|h| h.locked);
    out.handles = handles;
    out.locked_handles = locked;
    out
}

/// Parses and evaluates a source text.
pub fn eval_source(src: &str, opts: &EvalOptions) -> EvalOutput {
    let (file, mut diags) = cad_syntax::parse(src);
    let mut out = eval(src, &file, opts);
    diags.append(&mut out.diagnostics);
    out.diagnostics = diags;
    out
}

pub(crate) const MASK53: u64 = (1 << 53) - 1;

fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Order-dependent hash combination (stable across runs and platforms).
pub(crate) fn mix(a: u64, b: u64) -> u64 {
    splitmix(a.rotate_left(17) ^ splitmix(b))
}
