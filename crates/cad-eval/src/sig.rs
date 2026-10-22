//! Module signatures for the parameter bar.

use cad_syntax::ast::*;

use crate::output::{ModuleSig, ParamSig, ParamType};
use crate::value::PValue;

pub(crate) fn module_sigs(src: &str, file: &File) -> Vec<ModuleSig> {
    let mut out = Vec::new();
    let mut path = Vec::new();
    collect(src, &file.stmts, &mut path, &mut out);
    out
}

fn collect(src: &str, stmts: &[Stmt], path: &mut Vec<String>, out: &mut Vec<ModuleSig>) {
    for s in stmts {
        visit(src, s, path, out);
    }
}

fn visit(src: &str, s: &Stmt, path: &mut Vec<String>, out: &mut Vec<ModuleSig>) {
    match &s.kind {
        StmtKind::Module(m) => {
            out.push(sig(src, m, path));
            path.push(m.name.name.clone());
            collect(src, &m.body, path, out);
            path.pop();
        }
        StmtKind::Block(b) => collect(src, b, path, out),
        StmtKind::Call { children, .. } => collect(src, children, path, out),
        StmtKind::For { body, .. } | StmtKind::Let { body, .. } => visit(src, body, path, out),
        StmtKind::If { then, els, .. } => {
            visit(src, then, path, out);
            if let Some(e) = els {
                visit(src, e, path, out);
            }
        }
        _ => {}
    }
}

fn sig(src: &str, m: &ModuleDef, path: &[String]) -> ModuleSig {
    ModuleSig {
        name: m.name.name.clone(),
        stmt: m.stmt,
        span: m.span,
        name_span: m.name.span,
        body_span: m.body_span,
        path: path.to_vec(),
        params: m
            .params
            .iter()
            .map(|p| {
                let default_value = p.default.as_ref().and_then(const_value);
                ParamSig {
                    name: p.name.name.clone(),
                    span: p.span,
                    default_text: p.default.as_ref().and_then(|d| src.get(d.span.start as usize..d.span.end as usize)).map(String::from),
                    default_span: p.default.as_ref().map(|d| d.span),
                    ty: default_value.as_ref().map_or(ParamType::Unknown, type_of),
                    default_value,
                }
            })
            .collect(),
    }
}

/// Value of a constant literal expression.
pub(crate) fn const_value(e: &Expr) -> Option<PValue> {
    Some(match &e.kind {
        ExprKind::Number(n) => PValue::Num(*n),
        ExprKind::Unary(UnOp::Neg, x) => match &x.kind {
            ExprKind::Number(n) => PValue::Num(-n),
            _ => return None,
        },
        ExprKind::Bool(b) => PValue::Bool(*b),
        ExprKind::Str(s) => PValue::Str(s.clone()),
        ExprKind::Undef => PValue::Undef,
        ExprKind::Vector(el) => PValue::List(
            el.iter()
                .map(|v| match v {
                    VecElem::Expr(x) => const_value(x),
                    _ => None,
                })
                .collect::<Option<_>>()?,
        ),
        _ => return None,
    })
}

pub(crate) fn type_of(v: &PValue) -> ParamType {
    match v {
        PValue::Num(_) => ParamType::Number,
        PValue::Bool(_) => ParamType::Bool,
        PValue::Str(_) => ParamType::String,
        PValue::List(l) if (2..=3).contains(&l.len()) && l.iter().all(|x| matches!(x, PValue::Num(_))) => ParamType::Vector,
        PValue::List(_) => ParamType::List,
        _ => ParamType::Unknown,
    }
}
