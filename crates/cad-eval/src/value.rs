//! Runtime values. Every value carries its origin, which decides whether the UI can
//! edit it (handles) and what text to rewrite.
//!
//! Values borrow the syntax tree (`'a`): closures and lazy bindings point into the
//! parsed file instead of copying expressions.

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
///
/// Whether a literal was reached through a variable or a parameter default is not
/// stored here: it follows from comparing the literal's span with the expression at
/// the use site (see [`crate::Handle::via_var`]). That keeps variable access free of
/// copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Origin {
    /// Result of a computation: shown, not editable.
    #[default]
    Computed,
    /// A number or bool literal (a negated number counts as one literal) in the source.
    /// The value is exactly the literal's value.
    Literal(Span),
    /// Coordinate from a grid access expression (`g[a, b, c]`, `g.name`).
    GridRef(Span),
    /// Coordinate from an anchor expression `k.name`.
    AnchorRef { span: Span, inst: InstId, anchor: u32 },
}

impl Origin {
    pub fn span(&self) -> Option<Span> {
        match *self {
            Origin::Computed => None,
            Origin::Literal(s) | Origin::GridRef(s) | Origin::AnchorRef { span: s, .. } => Some(s),
        }
    }

    pub fn is_literal(&self) -> bool {
        matches!(self, Origin::Literal(_))
    }

    pub fn is_ref(&self) -> bool {
        matches!(self, Origin::GridRef(_) | Origin::AnchorRef { .. })
    }
}

#[derive(Debug)]
pub struct Closure<'a> {
    pub params: &'a [Param],
    pub body: &'a Expr,
    pub env: Env<'a>,
    pub name: Option<&'a str>,
}

/// Item of a grid access: a plane (index into the grid's plane list), `*`, or a
/// coordinate in the grid system (an expression that is not a plane name, as `t` in
/// `g[innen_l, vorne, t]`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Sel {
    Plane(u32),
    Wild,
    Coord(f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    pub start: f64,
    pub step: f64,
    pub end: f64,
}

impl Range {
    /// Number of values, following OpenSCAD 2021 (`floor((end - start) / step) + 1`,
    /// with the quotient moved up by one ulp first, so `[0 : 0.1 : 0.3]` has 4 values).
    pub fn count(&self) -> f64 {
        let Range { start, step, end } = *self;
        if !(start.is_finite() && step.is_finite() && end.is_finite()) || step == 0.0 {
            return 0.0;
        }
        if (step > 0.0 && start > end) || (step < 0.0 && start < end) {
            return 0.0;
        }
        ((end - start) / step).next_up().floor() + 1.0
    }

    pub fn at(&self, i: usize) -> f64 {
        self.start + i as f64 * self.step
    }
}

#[derive(Debug, Clone)]
pub enum Value<'a> {
    Undef,
    Bool(bool),
    Num(f64),
    Str(Rc<str>),
    List(Rc<Vec<Val<'a>>>),
    Range(Range),
    Func(Rc<Closure<'a>>),
    /// A grid name `g`; only usable in grid access expressions.
    Grid(GridId),
    /// `g[a, *, c]` / `g[*, *, c]`: a line (one wildcard) or plane (two wildcards).
    /// Points (no wildcard) evaluate to plain vectors with [`Origin::GridRef`].
    GridSel {
        grid: GridId,
        sel: Rc<[Sel; 3]>,
    },
    /// `g.name`: a plane that converts to its coordinate (in the grid system) in
    /// arithmetic and to a plane where an argument accepts one (`mirror`, `zone`).
    PlaneRef {
        grid: GridId,
        plane: u32,
        value: f64,
    },
    /// Named module instance `k`.
    Instance(InstId),
    /// `k` of `k[i]: m();`: a family of named instances, indexed by integers.
    Family(u32),
}

#[derive(Debug, Clone)]
pub struct Val<'a> {
    pub v: Value<'a>,
    pub origin: Origin,
    /// Span of the expression that produced this value, kept unchanged when the value
    /// is passed through variables, parameters and indexing. Used to rewrite a whole
    /// definition (snapping, replacing a grid reference by numbers).
    pub def: Option<Span>,
}

impl<'a> Val<'a> {
    pub fn new(v: Value<'a>) -> Self {
        Val { v, origin: Origin::Computed, def: None }
    }

    pub fn with_origin(v: Value<'a>, origin: Origin) -> Self {
        Val { v, origin, def: None }
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

    pub fn list(v: Vec<Val<'a>>) -> Self {
        Val::new(Value::List(Rc::new(v)))
    }

    pub fn vec3(v: DVec3) -> Self {
        Val::list(vec![Val::num(v.x), Val::num(v.y), Val::num(v.z)])
    }

    pub fn is_undef(&self) -> bool {
        matches!(self.v, Value::Undef)
    }

    /// Number, including a plane reference converted to its coordinate.
    pub fn as_num(&self) -> Option<f64> {
        match self.v {
            Value::Num(n) => Some(n),
            Value::PlaneRef { value, .. } => Some(value),
            _ => None,
        }
    }

    pub fn as_bool_strict(&self) -> Option<bool> {
        match self.v {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Val<'a>]> {
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

    /// List of numbers.
    pub fn as_nums(&self) -> Option<Vec<f64>> {
        self.as_list()?.iter().map(|v| v.as_num()).collect()
    }

    /// 2- or 3-vector of numbers as a 3D point (z = 0 for 2-vectors).
    pub fn as_vec3(&self) -> Option<DVec3> {
        let l = self.as_list()?;
        match l.len() {
            2 => Some(DVec3::new(l[0].as_num()?, l[1].as_num()?, 0.0)),
            3 => Some(DVec3::new(l[0].as_num()?, l[1].as_num()?, l[2].as_num()?)),
            _ => None,
        }
    }

    pub fn as_vec2(&self) -> Option<DVec2> {
        let l = self.as_list()?;
        if l.len() < 2 {
            return None;
        }
        Some(DVec2::new(l[0].as_num()?, l[1].as_num()?))
    }

    /// OpenSCAD truthiness (`nan` is true there).
    pub fn truthy(&self) -> bool {
        match &self.v {
            Value::Undef => false,
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Range(_) => true,
            Value::Func(_) => true,
            Value::PlaneRef { value, .. } => *value != 0.0,
            Value::Grid(_) | Value::GridSel { .. } | Value::Instance(_) | Value::Family(_) => true,
        }
    }

    /// Grids, grid lines/planes and instances: only allowed where an argument
    /// explicitly accepts them (a type error elsewhere).
    pub fn is_special(&self) -> bool {
        matches!(self.v, Value::Grid(_) | Value::GridSel { .. } | Value::Instance(_) | Value::Family(_))
    }

    pub fn type_name(&self) -> &'static str {
        match &self.v {
            Value::Undef => "undefined",
            Value::Bool(_) => "bool",
            Value::Num(_) => "number",
            Value::Str(_) => "string",
            Value::List(_) => "vector",
            Value::Range(_) => "range",
            Value::Func(_) => "function",
            Value::Grid(_) => "grid",
            Value::GridSel { sel, .. } => match sel.iter().filter(|s| **s == Sel::Wild).count() {
                1 => "grid line",
                _ => "grid plane",
            },
            Value::PlaneRef { .. } => "number",
            Value::Instance(_) => "instance",
            Value::Family(_) => "instance list",
        }
    }

    /// Type name for messages.
    pub fn type_de(&self) -> &'static str {
        match &self.v {
            Value::Undef => "undef",
            Value::Bool(_) => "Bool",
            Value::Num(_) | Value::PlaneRef { .. } => "Zahl",
            Value::Str(_) => "String",
            Value::List(_) => "Vektor",
            Value::Range(_) => "Bereich",
            Value::Func(_) => "Funktion",
            Value::Grid(_) => "Raster",
            Value::GridSel { sel, .. } => match sel.iter().filter(|s| **s == Sel::Wild).count() {
                1 => "Rasterlinie",
                _ => "Rasterebene",
            },
            Value::Instance(_) => "Instanz",
            Value::Family(_) => "Instanzliste",
        }
    }
}

/// A plain value without references into the evaluation (output, overrides).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PValue {
    Undef,
    Bool(bool),
    Num(f64),
    Str(String),
    List(Vec<PValue>),
    Range {
        start: f64,
        step: f64,
        end: f64,
    },
    Function,
    /// Grids, grid lines/planes, instances: shown by name only.
    Other(String),
}

impl PValue {
    pub fn to_val<'a>(&self) -> Val<'a> {
        match self {
            PValue::Undef | PValue::Function | PValue::Other(_) => Val::undef(),
            PValue::Bool(b) => Val::bool(*b),
            PValue::Num(n) => Val::num(*n),
            PValue::Str(s) => Val::str(s),
            PValue::List(l) => Val::list(l.iter().map(|p| p.to_val()).collect()),
            PValue::Range { start, step, end } => {
                Val::new(Value::Range(Range { start: *start, step: *step, end: *end }))
            }
        }
    }

    pub fn from_val(v: &Val) -> PValue {
        match &v.v {
            Value::Undef => PValue::Undef,
            Value::Bool(b) => PValue::Bool(*b),
            Value::Num(n) => PValue::Num(*n),
            Value::PlaneRef { value, .. } => PValue::Num(*value),
            Value::Str(s) => PValue::Str(s.to_string()),
            Value::List(l) => PValue::List(l.iter().map(PValue::from_val).collect()),
            Value::Range(r) => PValue::Range { start: r.start, step: r.step, end: r.end },
            Value::Func(_) => PValue::Function,
            Value::Grid(_) | Value::GridSel { .. } | Value::Instance(_) | Value::Family(_) => {
                PValue::Other(v.type_name().into())
            }
        }
    }
}

/// Formats a number like OpenSCAD 2021 (6 significant digits, exponent form outside
/// 1e-5 ..< 1e6, e.g. `1.23457e+6`, `1e-7`). Exact ties round away from zero as in
/// OpenSCAD (`123456.5` → `123457`), not to even.
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
    let n = if is_tie6(n) { if n > 0.0 { n.next_up() } else { n.next_down() } } else { n };
    let s = format!("{n:.5e}");
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let x: i32 = exp.parse().unwrap_or(0);
    if !(-5..6).contains(&x) {
        let m = trim_zeros(mant);
        format!("{m}e{}{}", if x < 0 { "-" } else { "+" }, x.abs())
    } else {
        let decimals = (5 - x) as usize;
        trim_zeros(&format!("{n:.decimals$}"))
    }
}

/// The exact decimal value of `n` lies exactly halfway between two 6-digit values.
fn is_tie6(n: f64) -> bool {
    // A double that is not a tie differs from one within far fewer than 40 digits.
    let s = format!("{:.40e}", n.abs());
    let mant = s.split_once('e').map_or(s.as_str(), |(m, _)| m);
    let digits = mant.as_bytes();
    // "d.ddddd" are the 6 kept digits; index 7 is the 7th significant digit.
    digits.get(7) == Some(&b'5') && digits[8..].iter().all(|&c| c == b'0')
}

fn trim_zeros(s: &str) -> String {
    let t = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { s };
    if t == "-0" { "0".into() } else { t.into() }
}

/// Number of decimals in the shortest representation of a rounding step.
fn step_decimals(step: f64) -> usize {
    let s = format!("{step}");
    match s.split_once('.') {
        Some((_, d)) => d.len().min(10),
        None => 0,
    }
}

/// Formats a number for writing back into source: rounded to a multiple of `step`
/// (no rounding for `step <= 0`, then at most 6 decimals), shortest form, no trailing
/// zeros, never `-0`.
pub fn fmt_literal(n: f64, step: f64) -> String {
    if !n.is_finite() {
        return "0".into();
    }
    let (v, decimals) = if step > 0.0 { ((n / step).round() * step, step_decimals(step)) } else { (n, 6) };
    trim_zeros(&format!("{v:.decimals$}"))
}

/// Display options for [`display`].
#[derive(Debug, Clone, Copy)]
pub struct DisplayOpts {
    /// Strings at the top level are written without quotes (`str()`).
    pub raw_top: bool,
    /// Maximum list elements written per list (rest as `…`).
    pub max_elems: usize,
}

impl DisplayOpts {
    pub const FULL: DisplayOpts = DisplayOpts { raw_top: false, max_elems: usize::MAX };
}

pub fn display(v: &Val, out: &mut String, top: bool, o: DisplayOpts) {
    match &v.v {
        Value::Undef => out.push_str("undef"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Num(n) => out.push_str(&fmt_num(*n)),
        Value::PlaneRef { value, .. } => out.push_str(&fmt_num(*value)),
        Value::Str(s) => {
            if top && o.raw_top {
                out.push_str(s)
            } else {
                out.push('"');
                out.push_str(s);
                out.push('"');
            }
        }
        Value::List(l) => {
            out.push('[');
            for (i, e) in l.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                if i >= o.max_elems {
                    out.push('…');
                    break;
                }
                display(e, out, false, o);
            }
            out.push(']');
        }
        Value::Range(r) => {
            let _ = write!(out, "[{} : {} : {}]", fmt_num(r.start), fmt_num(r.step), fmt_num(r.end));
        }
        Value::Func(c) => crate::print::function(c.params, c.body, out),
        Value::Grid(_) | Value::GridSel { .. } | Value::Instance(_) | Value::Family(_) => out.push_str(v.type_name()),
    }
}

/// OpenSCAD `echo` representation (strings quoted).
pub fn to_display(v: &Val) -> String {
    let mut s = String::new();
    display(v, &mut s, true, DisplayOpts::FULL);
    s
}

/// OpenSCAD `str()` representation (top-level strings unquoted).
pub fn to_str(v: &Val) -> String {
    let mut s = String::new();
    display(v, &mut s, true, DisplayOpts { raw_top: true, max_elems: usize::MAX });
    s
}

/// Short representation for inline hints.
pub fn to_hint(v: &Val) -> String {
    let mut s = String::new();
    display(v, &mut s, true, DisplayOpts { raw_top: false, max_elems: 6 });
    if let Value::List(l) = &v.v
        && l.len() > 6
    {
        let _ = write!(s, " ({})", l.len());
    }
    if s.chars().count() > 80 {
        let t: String = s.chars().take(79).collect();
        return t + "…";
    }
    s
}

/// Structural equality (OpenSCAD `==`).
pub fn values_eq(a: &Val, b: &Val) -> bool {
    if let (Some(x), Some(y)) = (num_only(a), num_only(b)) {
        return x == y;
    }
    match (&a.v, &b.v) {
        (Value::Undef, Value::Undef) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::List(x), Value::List(y)) => x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| values_eq(p, q)),
        (Value::Range(x), Value::Range(y)) => x == y,
        (Value::Grid(x), Value::Grid(y)) => x == y,
        (Value::Instance(x), Value::Instance(y)) => x == y,
        (Value::Family(x), Value::Family(y)) => x == y,
        (Value::GridSel { grid: g1, sel: s1 }, Value::GridSel { grid: g2, sel: s2 }) => g1 == g2 && s1 == s2,
        _ => false,
    }
}

fn num_only(v: &Val) -> Option<f64> {
    match v.v {
        Value::Num(n) => Some(n),
        Value::PlaneRef { value, .. } => Some(value),
        _ => None,
    }
}
