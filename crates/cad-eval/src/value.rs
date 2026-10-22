//! Runtime values. Every value carries its origin, which decides whether the UI can
//! edit it (handles) and what text to rewrite.

use std::fmt::Write;
use std::rc::Rc;

use cad_syntax::Span;
use cad_syntax::ast::{Expr, Param};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

use crate::env::Env;

pub type GridId = u32;
pub type InstId = u32;

/// Where a value comes from.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum Origin {
    /// Result of a computation: shown, not editable.
    #[default]
    Computed,
    /// A number literal (possibly negated) in the source.
    Literal {
        span: Span,
        /// Reached through a variable (editing changes its definition).
        via_var: bool,
        /// Default value of a module parameter (editing changes all calls).
        via_default: bool,
    },
    /// A grid point / line / plane reference expression.
    GridRef { span: Span, via_var: bool },
    /// `k.anchor` expression.
    AnchorRef { span: Span, via_var: bool },
}

impl Origin {
    pub fn literal(span: Span) -> Self {
        Origin::Literal { span, via_var: false, via_default: false }
    }

    pub fn is_editable(&self) -> bool {
        matches!(self, Origin::Literal { .. })
    }

    pub fn span(&self) -> Option<Span> {
        match self {
            Origin::Computed => None,
            Origin::Literal { span, .. } | Origin::GridRef { span, .. } | Origin::AnchorRef { span, .. } => {
                Some(*span)
            }
        }
    }

    pub fn through_var(&self) -> Origin {
        match self.clone() {
            Origin::Literal { span, via_default, .. } => Origin::Literal { span, via_var: true, via_default },
            Origin::GridRef { span, .. } => Origin::GridRef { span, via_var: true },
            Origin::AnchorRef { span, .. } => Origin::AnchorRef { span, via_var: true },
            Origin::Computed => Origin::Computed,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Closure {
    pub params: Vec<Param>,
    pub body: Rc<Expr>,
    pub env: Env,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sel {
    /// Plane index into the grid's plane list.
    Plane(usize),
    /// `*`
    Wild,
}

#[derive(Debug, Clone)]
pub enum Value {
    Undef,
    Bool(bool),
    Num(f64),
    Str(Rc<str>),
    List(Rc<Vec<Val>>),
    Range { start: f64, step: f64, end: f64 },
    Func(Rc<Closure>),
    /// `g[a, b, c]` with zero, one or two wildcards: point, line or plane.
    GridSel { grid: GridId, sel: [Sel; 3] },
    /// `g.name`: a plane that converts to its coordinate in arithmetic.
    PlaneRef { grid: GridId, plane: usize },
    /// Named module instance `k`.
    Instance(InstId),
    /// `k.anchor`: point with optional direction, in the current module system.
    Anchor { inst: InstId, name: Rc<str>, point: DVec3, dir: Option<DVec3> },
}

#[derive(Debug, Clone)]
pub struct Val {
    pub v: Value,
    pub origin: Origin,
}

impl Val {
    pub fn new(v: Value) -> Self {
        Val { v, origin: Origin::Computed }
    }

    pub fn with_origin(v: Value, origin: Origin) -> Self {
        Val { v, origin }
    }

    pub fn undef() -> Self {
        Val::new(Value::Undef)
    }

    pub fn num(n: f64) -> Self {
        Val::new(Value::Num(n))
    }

    pub fn bool(b: bool) -> Self {
        Val::new(Value::Bool(b))
    }

    pub fn str(s: &str) -> Self {
        Val::new(Value::Str(s.into()))
    }

    pub fn list(v: Vec<Val>) -> Self {
        Val::new(Value::List(Rc::new(v)))
    }

    pub fn vec3(v: DVec3) -> Self {
        Val::list(vec![Val::num(v.x), Val::num(v.y), Val::num(v.z)])
    }

    pub fn is_undef(&self) -> bool {
        matches!(self.v, Value::Undef)
    }

    pub fn as_num(&self) -> Option<f64> {
        match self.v {
            Value::Num(n) => Some(n),
            _ => None,
        }
    }

    pub fn as_bool_strict(&self) -> Option<bool> {
        match self.v {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Val]> {
        match &self.v {
            Value::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match &self.v {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// OpenSCAD truthiness.
    pub fn truthy(&self) -> bool {
        match &self.v {
            Value::Undef => false,
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0 && !n.is_nan(),
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Range { .. } => true,
            Value::Func(_) => true,
            Value::GridSel { .. } | Value::PlaneRef { .. } | Value::Instance(_) | Value::Anchor { .. } => true,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match &self.v {
            Value::Undef => "undef",
            Value::Bool(_) => "bool",
            Value::Num(_) => "number",
            Value::Str(_) => "string",
            Value::List(_) => "list",
            Value::Range { .. } => "range",
            Value::Func(_) => "function",
            Value::GridSel { sel, .. } => match sel.iter().filter(|s| **s == Sel::Wild).count() {
                0 => "grid point",
                1 => "grid line",
                2 => "grid plane",
                _ => "grid",
            },
            Value::PlaneRef { .. } => "grid plane",
            Value::Instance(_) => "instance",
            Value::Anchor { .. } => "anchor",
        }
    }
}

/// Formats a number like OpenSCAD's `str()` (up to 6 significant digits).
pub fn fmt_num(n: f64) -> String {
    if n.is_nan() {
        return "nan".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if n == 0.0 {
        return "0".into();
    }
    let a = n.abs();
    if (1e-5..1e16).contains(&a) {
        let digits = (6 - 1 - a.log10().floor() as i32).max(0) as usize;
        let s = format!("{n:.digits$}");
        trim_zeros(&s)
    } else {
        let s = format!("{n:.5e}");
        // 1.00000e-6 -> 1e-06 style
        let (m, e) = s.split_once('e').unwrap();
        let m = trim_zeros(m);
        let e: i32 = e.parse().unwrap_or(0);
        format!("{m}e{}{:02}", if e < 0 { "-" } else { "+" }, e.abs())
    }
}

fn trim_zeros(s: &str) -> String {
    if s.contains('.') {
        let t = s.trim_end_matches('0').trim_end_matches('.');
        if t == "-0" { "0".into() } else { t.into() }
    } else {
        s.into()
    }
}

/// Formats a number for writing back into source: rounded to `step`, shortest form.
pub fn fmt_literal(n: f64, step: f64) -> String {
    let v = if step > 0.0 { (n / step).round() * step } else { n };
    let decimals = if step > 0.0 && step < 1.0 { (-step.log10()).ceil().max(0.0) as usize + 1 } else { 0 };
    let s = format!("{v:.decimals$}");
    trim_zeros(&s)
}

pub fn display(v: &Val, out: &mut String, top: bool) {
    match &v.v {
        Value::Undef => out.push_str("undef"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Num(n) => out.push_str(&fmt_num(*n)),
        Value::Str(s) => {
            if top {
                out.push_str(s)
            } else {
                let _ = write!(out, "{s:?}");
            }
        }
        Value::List(l) => {
            out.push('[');
            for (i, e) in l.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                display(e, out, false);
            }
            out.push(']');
        }
        Value::Range { start, step, end } => {
            let _ = write!(out, "[{} : {} : {}]", fmt_num(*start), fmt_num(*step), fmt_num(*end));
        }
        Value::Func(_) => out.push_str("function"),
        Value::GridSel { .. } => out.push_str(v.type_name()),
        Value::PlaneRef { .. } => out.push_str("grid plane"),
        Value::Instance(_) => out.push_str("instance"),
        Value::Anchor { point, .. } => {
            let _ = write!(out, "[{}, {}, {}]", fmt_num(point.x), fmt_num(point.y), fmt_num(point.z));
        }
    }
}

pub fn to_display(v: &Val) -> String {
    let mut s = String::new();
    display(v, &mut s, false);
    s
}

pub fn to_str(v: &Val) -> String {
    let mut s = String::new();
    display(v, &mut s, true);
    s
}

pub fn num_eq(a: f64, b: f64) -> bool {
    a == b
}

/// Structural equality (OpenSCAD `==`).
pub fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Undef, Value::Undef) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Num(x), Value::Num(y)) => num_eq(*x, *y),
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| values_eq(&p.v, &q.v))
        }
        (Value::Range { start: a, step: b, end: c }, Value::Range { start: d, step: e, end: f }) => {
            a == d && b == e && c == f
        }
        _ => false,
    }
}

pub fn vec2_of(v: &Val) -> Option<DVec2> {
    let l = v.as_list()?;
    if l.len() < 2 {
        return None;
    }
    Some(DVec2::new(l[0].as_num()?, l[1].as_num()?))
}
