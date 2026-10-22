#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub mod bounds;

use cad_eval::{EvalOptions, EvalOutput, eval_source};
use cad_ir::{NodeIdx, Op, Tree};
use cad_syntax::Severity;

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// `.scad` files of a directory below the workspace root, sorted by name.
pub fn scad_files(dir: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(workspace_root().join(dir))
        .unwrap_or_else(|e| panic!("{dir}: {e}"))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "scad"))
        .collect();
    v.sort();
    v
}

pub fn file_name(p: &Path) -> String {
    p.file_name().unwrap().to_string_lossy().into_owned()
}

/// FNV-1a 64 over the source bytes; same as `stl_stats.fnv1a64` in Python.
pub fn fnv1a64(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// 1-based line and column of a byte offset.
pub fn line_col(src: &str, byte: u32) -> (usize, usize) {
    let pre = &src[..(byte as usize).min(src.len())];
    let line = pre.matches('\n').count() + 1;
    let col = pre.len() - pre.rfind('\n').map_or(0, |i| i + 1) + 1;
    (line, col)
}

pub fn eval(src: &str) -> EvalOutput {
    eval_source(src, &EvalOptions::default())
}

pub fn diags(src: &str, out: &EvalOutput, sev: Severity) -> Vec<String> {
    out.diagnostics
        .iter()
        .filter(|d| d.severity == sev)
        .map(|d| {
            let (l, c) = line_col(src, d.span.start);
            format!("{l}:{c}: {}", d.message)
        })
        .collect()
}

/// Solid geometry leaves (primitives, profiles, profile operations) that contribute to
/// the result: reachable from the root through non-disabled, non-background, non-zone
/// nodes.
pub fn solid_leaves(tree: &Tree) -> usize {
    fn walk(t: &Tree, i: NodeIdx, n: &mut usize) {
        match &t.node(i).op {
            Op::Prim { .. }
            | Op::Profile(_)
            | Op::LinearExtrude { .. }
            | Op::RotateExtrude { .. }
            | Op::Loft { .. }
            | Op::Sweep { .. }
            | Op::Surface { .. } => *n += 1,
            _ => {}
        }
        for c in t.solid_children(i) {
            walk(t, c, n);
        }
    }
    let mut n = 0;
    if let Some(r) = tree.root {
        walk(tree, r, &mut n);
    }
    n
}

/// Generic stage-1 problems of an evaluation: errors, abort, empty or 2D-only result.
pub fn stage1_problems(src: &str, out: &EvalOutput) -> Vec<String> {
    let mut p: Vec<String> = diags(src, out, Severity::Error).into_iter().map(|e| format!("error {e}")).collect();
    if out.aborted {
        p.push("evaluation aborted".into());
    }
    match out.tree.root {
        None => p.push("tree has no root".into()),
        Some(r) => {
            if solid_leaves(&out.tree) == 0 {
                p.push("tree has no solid geometry".into());
            } else if out.tree.dim(r) != Some(cad_ir::Dim::Three) {
                p.push(format!("result is not 3D ({:?})", out.tree.dim(r)));
            }
        }
    }
    p
}
