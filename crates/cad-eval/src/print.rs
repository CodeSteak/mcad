//! Source form of expressions as OpenSCAD 2021 prints function literals
//! (`echo(function(x) x * 2)` → `function(x) (x * 2)`): every binary and ternary
//! operation in parentheses, numbers in the 6-digit format, strings escaped.

use std::fmt::Write;

use cad_syntax::ast::{Arg, Assignment, BinOp, Expr, ExprKind, IndexItem, Param, UnOp, VecElem};

use crate::value::fmt_num;

/// `function(params) body`.
pub fn function(params: &[Param], body: &Expr, out: &mut String) {
    out.push_str("function(");
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&p.name.name);
        if let Some(d) = &p.default {
            out.push_str(" = ");
            expr(d, out);
        }
    }
    out.push_str(") ");
    expr(body, out);
}

fn bin_sym(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Pow => "^",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

fn string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn args(a: &[Arg], out: &mut String) {
    for (i, a) in a.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        if let Some(n) = &a.name {
            out.push_str(&n.name);
            out.push_str(" = ");
        }
        expr(&a.value, out);
    }
}

fn assigns(a: &[Assignment], sep: &str, out: &mut String) {
    for (i, a) in a.iter().enumerate() {
        if i > 0 {
            out.push_str(sep);
        }
        out.push_str(&a.name.name);
        out.push_str(" = ");
        expr(&a.value, out);
    }
}

pub fn expr(e: &Expr, out: &mut String) {
    match &e.kind {
        ExprKind::Number(n) => out.push_str(&fmt_num(*n)),
        ExprKind::Str(s) => string(s, out),
        ExprKind::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        ExprKind::Undef => out.push_str("undef"),
        ExprKind::Ident(n) => out.push_str(n),
        ExprKind::Unary(op, x) => {
            match op {
                UnOp::Neg => out.push('-'),
                UnOp::Not => out.push('!'),
                UnOp::Plus => {}
            }
            expr(x, out);
        }
        ExprKind::Binary(op, a, b) => {
            out.push('(');
            expr(a, out);
            let _ = write!(out, " {} ", bin_sym(*op));
            expr(b, out);
            out.push(')');
        }
        ExprKind::Ternary(c, a, b) => {
            out.push('(');
            expr(c, out);
            out.push_str(" ? ");
            expr(a, out);
            out.push_str(" : ");
            expr(b, out);
            out.push(')');
        }
        ExprKind::Call { callee, args: a } => {
            expr(callee, out);
            out.push('(');
            args(a, out);
            out.push(')');
        }
        ExprKind::Index { base, items } => {
            expr(base, out);
            out.push('[');
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                match it {
                    IndexItem::Expr(x) => expr(x, out),
                    IndexItem::Wild(_) => out.push('*'),
                }
            }
            out.push(']');
        }
        ExprKind::Member { base, name } => {
            expr(base, out);
            out.push('.');
            out.push_str(&name.name);
        }
        ExprKind::Vector(elems) => {
            out.push('[');
            for (i, el) in elems.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                vec_elem(el, out);
            }
            out.push(']');
        }
        ExprKind::Range { start, step, end } => {
            out.push('[');
            expr(start, out);
            if let Some(s) = step {
                out.push_str(" : ");
                expr(s, out);
            }
            out.push_str(" : ");
            expr(end, out);
            out.push(']');
        }
        ExprKind::FuncLit { params, body } => function(params, body, out),
        ExprKind::Let { vars, body } => {
            out.push_str("let(");
            assigns(vars, ", ", out);
            out.push_str(") ");
            expr(body, out);
        }
        ExprKind::Assert { args: a, body } | ExprKind::Echo { args: a, body } => {
            out.push_str(if matches!(e.kind, ExprKind::Assert { .. }) { "assert(" } else { "echo(" });
            args(a, out);
            out.push(')');
            if let Some(b) = body {
                out.push(' ');
                expr(b, out);
            }
        }
        ExprKind::Error => {}
    }
}

/// List comprehension elements; OpenSCAD wraps their bodies in parentheses and
/// writes `for (i = a, j = b)` as nested `for`s.
fn vec_elem(el: &VecElem, out: &mut String) {
    match el {
        VecElem::Expr(e) => expr(e, out),
        VecElem::For { vars, body, .. } => {
            for v in vars {
                out.push_str("for(");
                out.push_str(&v.name.name);
                out.push_str(" = ");
                expr(&v.value, out);
                out.push_str(") (");
            }
            vec_elem(body, out);
            for _ in vars {
                out.push(')');
            }
        }
        VecElem::ForC { init, cond, update, body, .. } => {
            out.push_str("for(");
            assigns(init, ", ", out);
            out.push(';');
            expr(cond, out);
            out.push(';');
            assigns(update, ", ", out);
            out.push_str(") ");
            vec_elem(body, out);
        }
        VecElem::If { cond, then, els, .. } => {
            out.push_str("if(");
            expr(cond, out);
            out.push_str(") (");
            vec_elem(then, out);
            out.push(')');
            if let Some(e) = els {
                out.push_str(" else (");
                vec_elem(e, out);
                out.push(')');
            }
        }
        VecElem::Let { vars, body, .. } => {
            out.push_str("let(");
            assigns(vars, ", ", out);
            out.push_str(") ");
            vec_elem(body, out);
        }
        VecElem::Each(inner, _) => {
            out.push_str("each (");
            vec_elem(inner, out);
            out.push(')');
        }
    }
}
