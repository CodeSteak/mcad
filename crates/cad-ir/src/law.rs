//! Radius laws: a fillet radius that is a function of the position.
//!
//! `fillet(r = function (p) p.z < 10 ? 2 : 4) { … }` hands the engine a function it has to
//! evaluate at points it only knows after the kernel has built the body (the stations along
//! each selected edge). The evaluator therefore compiles the function literal into a [`Law`]:
//! a small pure expression over the position `p`, in which everything that does not depend
//! on `p` (captured variables, constant sub-expressions, list comprehensions) is already a
//! value and calls of user functions are inlined. The engine evaluates it with [`Law::eval`].
//! A law is plain data: part of the tree, serialisable, and part of the node's cache key
//! (equal laws give equal keys, a changed captured variable a new one).
//!
//! Semantics follow the language (OpenSCAD) for numbers, booleans, vectors and `undef`:
//! elementwise `+`/`-`, scalar and dot products, comparisons, truthiness, indexing, `p.x`,
//! and the math built-ins listed in [`LawFn`] (angles in degrees).

use glam::DVec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Law {
    pub expr: LawExpr,
    /// Local slots used by `Let` (let variables, parameters of inlined calls).
    pub slots: u32,
    /// Source text of the function literal (display).
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LawExpr {
    Undef,
    Bool(bool),
    Num(f64),
    /// The position `p` = `[x, y, z]`.
    Point,
    Slot(u32),
    List(Vec<LawExpr>),
    Neg(Box<LawExpr>),
    Not(Box<LawExpr>),
    Bin(LawOp, Box<LawExpr>, Box<LawExpr>),
    And(Box<LawExpr>, Box<LawExpr>),
    Or(Box<LawExpr>, Box<LawExpr>),
    Cond(Box<LawExpr>, Box<LawExpr>, Box<LawExpr>),
    Index(Box<LawExpr>, Box<LawExpr>),
    Call(LawFn, Vec<LawExpr>),
    /// `slot = value; body`
    Let(u32, Box<LawExpr>, Box<LawExpr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LawOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LawFn {
    Abs,
    Sign,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Atan2,
    Floor,
    Ceil,
    Round,
    Ln,
    Log,
    Exp,
    Sqrt,
    Pow,
    Min,
    Max,
    Norm,
    Len,
    Lookup,
}

impl LawFn {
    /// The built-in of this name usable in a law.
    pub fn from_name(name: &str) -> Option<LawFn> {
        Some(match name {
            "abs" => LawFn::Abs,
            "sign" => LawFn::Sign,
            "sin" => LawFn::Sin,
            "cos" => LawFn::Cos,
            "tan" => LawFn::Tan,
            "asin" => LawFn::Asin,
            "acos" => LawFn::Acos,
            "atan" => LawFn::Atan,
            "atan2" => LawFn::Atan2,
            "floor" => LawFn::Floor,
            "ceil" => LawFn::Ceil,
            "round" => LawFn::Round,
            "ln" => LawFn::Ln,
            "log" => LawFn::Log,
            "exp" => LawFn::Exp,
            "sqrt" => LawFn::Sqrt,
            "pow" => LawFn::Pow,
            "min" => LawFn::Min,
            "max" => LawFn::Max,
            "norm" => LawFn::Norm,
            "len" => LawFn::Len,
            "lookup" => LawFn::Lookup,
            _ => return None,
        })
    }
}

/// Value during the evaluation of a law.
#[derive(Debug, Clone, PartialEq)]
pub enum LawVal {
    Undef,
    Bool(bool),
    Num(f64),
    List(Vec<LawVal>),
}

impl LawVal {
    fn num(&self) -> Option<f64> {
        match self {
            LawVal::Num(x) => Some(*x),
            _ => None,
        }
    }

    fn truthy(&self) -> bool {
        match self {
            LawVal::Undef => false,
            LawVal::Bool(b) => *b,
            LawVal::Num(x) => *x != 0.0,
            LawVal::List(l) => !l.is_empty(),
        }
    }

    fn nums(&self) -> Option<Vec<f64>> {
        match self {
            LawVal::List(l) => l.iter().map(|v| v.num()).collect(),
            _ => None,
        }
    }

    /// Short description for error messages.
    pub fn describe(&self) -> String {
        match self {
            LawVal::Undef => "undef".into(),
            LawVal::Bool(b) => b.to_string(),
            LawVal::Num(x) => format!("{x}"),
            LawVal::List(l) => format!("[{}]", l.iter().map(|v| v.describe()).collect::<Vec<_>>().join(", ")),
        }
    }
}

/// Degree trigonometry, exact at multiples of 90° (as the language's `sin`/`cos`).
fn sin_deg(x: f64) -> f64 {
    let r = x.rem_euclid(360.0);
    if r == 0.0 || r == 180.0 {
        0.0
    } else if r == 90.0 {
        1.0
    } else if r == 270.0 {
        -1.0
    } else {
        r.to_radians().sin()
    }
}

fn cos_deg(x: f64) -> f64 {
    sin_deg(x + 90.0)
}

fn addsub(a: &LawVal, b: &LawVal, sub: bool) -> LawVal {
    match (a, b) {
        (LawVal::Num(x), LawVal::Num(y)) => LawVal::Num(if sub { x - y } else { x + y }),
        (LawVal::List(x), LawVal::List(y)) => LawVal::List(x.iter().zip(y).map(|(p, q)| addsub(p, q, sub)).collect()),
        _ => LawVal::Undef,
    }
}

fn scale(l: &[LawVal], k: f64, f: &dyn Fn(f64, f64) -> f64) -> LawVal {
    LawVal::List(
        l.iter()
            .map(|e| match e {
                LawVal::Num(n) => LawVal::Num(f(*n, k)),
                LawVal::List(inner) => scale(inner, k, f),
                _ => LawVal::Undef,
            })
            .collect(),
    )
}

fn compare(a: &LawVal, b: &LawVal) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    match (a, b) {
        (LawVal::Num(x), LawVal::Num(y)) => x.partial_cmp(y),
        (LawVal::Bool(x), LawVal::Bool(y)) => Some(x.cmp(y)),
        (LawVal::List(x), LawVal::List(y)) => {
            for (p, q) in x.iter().zip(y) {
                match compare(p, q)? {
                    Ordering::Equal => continue,
                    o => return Some(o),
                }
            }
            Some(x.len().cmp(&y.len()))
        }
        _ => None,
    }
}

fn binop(op: LawOp, a: &LawVal, b: &LawVal) -> LawVal {
    use LawVal::*;
    use std::cmp::Ordering;
    match op {
        LawOp::Add | LawOp::Sub => addsub(a, b, op == LawOp::Sub),
        LawOp::Mul => match (a, b) {
            (Num(x), Num(y)) => Num(x * y),
            (List(l), Num(k)) | (Num(k), List(l)) => scale(l, *k, &|n, k| n * k),
            (List(_), List(_)) => match (a.nums(), b.nums()) {
                (Some(u), Some(v)) if u.len() == v.len() && !u.is_empty() => {
                    Num(u.iter().zip(&v).map(|(p, q)| p * q).sum())
                }
                _ => Undef,
            },
            _ => Undef,
        },
        LawOp::Div => match (a, b) {
            (Num(x), Num(y)) => Num(x / y),
            (List(l), Num(k)) => scale(l, *k, &|n, k| n / k),
            (Num(k), List(l)) => scale(l, *k, &|n, k| k / n),
            _ => Undef,
        },
        LawOp::Mod => match (a, b) {
            (Num(x), Num(y)) => Num(x % y),
            _ => Undef,
        },
        LawOp::Pow => match (a, b) {
            (Num(x), Num(y)) => Num(x.powf(*y)),
            _ => Undef,
        },
        LawOp::Eq => Bool(a == b),
        LawOp::Ne => Bool(a != b),
        LawOp::Lt | LawOp::Le | LawOp::Gt | LawOp::Ge => match compare(a, b) {
            None => Bool(false),
            Some(o) => Bool(match op {
                LawOp::Lt => o == Ordering::Less,
                LawOp::Le => o != Ordering::Greater,
                LawOp::Gt => o == Ordering::Greater,
                _ => o != Ordering::Less,
            }),
        },
    }
}

fn lookup(key: f64, table: &LawVal) -> LawVal {
    let LawVal::List(t) = table else { return LawVal::Undef };
    let pairs: Vec<(f64, f64)> = t
        .iter()
        .filter_map(|e| match e.nums() {
            Some(v) if v.len() >= 2 => Some((v[0], v[1])),
            _ => None,
        })
        .collect();
    let Some(&first) = pairs.first() else { return LawVal::Undef };
    let (mut low, mut high) = (first, first);
    for &(tp, tv) in &pairs[1..] {
        if tp <= key && (tp > low.0 || low.0 > key) {
            low = (tp, tv);
        }
        if tp >= key && (tp < high.0 || high.0 < key) {
            high = (tp, tv);
        }
    }
    if key <= low.0 {
        return LawVal::Num(high.1);
    }
    if key >= high.0 {
        return LawVal::Num(low.1);
    }
    let f = (key - low.0) / (high.0 - low.0);
    LawVal::Num(high.1 * f + low.1 * (1.0 - f))
}

fn call(f: LawFn, a: &[LawVal]) -> LawVal {
    use LawVal::*;
    let n = |i: usize| a.get(i).and_then(|v| v.num());
    let num1 = |g: fn(f64) -> f64| n(0).map_or(Undef, |x| Num(g(x)));
    match f {
        LawFn::Abs => num1(f64::abs),
        LawFn::Sign => num1(|x| {
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            }
        }),
        LawFn::Sin => num1(sin_deg),
        LawFn::Cos => num1(cos_deg),
        LawFn::Tan => num1(|x| sin_deg(x) / cos_deg(x)),
        LawFn::Asin => num1(|x| x.asin().to_degrees()),
        LawFn::Acos => num1(|x| x.acos().to_degrees()),
        LawFn::Atan => num1(|x| x.atan().to_degrees()),
        LawFn::Atan2 => match (n(0), n(1)) {
            (Some(y), Some(x)) => Num(y.atan2(x).to_degrees()),
            _ => Undef,
        },
        LawFn::Floor => num1(f64::floor),
        LawFn::Ceil => num1(f64::ceil),
        LawFn::Round => num1(f64::round),
        LawFn::Ln => num1(f64::ln),
        LawFn::Log => match (n(0), n(1)) {
            (Some(b), Some(x)) => Num(x.ln() / b.ln()),
            (Some(x), None) => Num(x.log10()),
            _ => Undef,
        },
        LawFn::Exp => num1(f64::exp),
        LawFn::Sqrt => num1(f64::sqrt),
        LawFn::Pow => match (n(0), n(1)) {
            (Some(x), Some(y)) => Num(x.powf(y)),
            _ => Undef,
        },
        LawFn::Min | LawFn::Max => {
            let vals: Option<Vec<f64>> = if a.len() == 1 { a[0].nums() } else { a.iter().map(|v| v.num()).collect() };
            match vals {
                Some(v) if !v.is_empty() => Num(v.into_iter().fold(
                    if f == LawFn::Max { f64::NEG_INFINITY } else { f64::INFINITY },
                    |acc, x| if f == LawFn::Max { acc.max(x) } else { acc.min(x) },
                )),
                _ => Undef,
            }
        }
        LawFn::Norm => match a.first().and_then(|v| v.nums()) {
            Some(v) => Num(v.iter().map(|x| x * x).sum::<f64>().sqrt()),
            None => Undef,
        },
        LawFn::Len => match a.first() {
            Some(List(l)) => Num(l.len() as f64),
            _ => Undef,
        },
        LawFn::Lookup => match (n(0), a.get(1)) {
            (Some(k), Some(t)) if k.is_finite() => lookup(k, t),
            _ => Undef,
        },
    }
}

impl Law {
    /// Value of the law at `p`.
    pub fn value(&self, p: DVec3) -> LawVal {
        let mut slots = vec![LawVal::Undef; self.slots as usize];
        eval(&self.expr, p, &mut slots)
    }

    /// Radius at `p`: the law's value must be a number (any number; the caller checks > 0).
    pub fn eval(&self, p: DVec3) -> Result<f64, String> {
        match self.value(p) {
            LawVal::Num(x) => Ok(x),
            v => Err(format!("Radiusfunktion liefert keine Zahl ({}) bei {}", v.describe(), fmt_point(p))),
        }
    }
}

/// `[x, y, z]` with up to 3 decimals (diagnostics).
pub fn fmt_point(p: DVec3) -> String {
    let f = |x: f64| {
        let s = format!("{:.3}", x);
        let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        if s == "-0" { "0".to_string() } else { s }
    };
    format!("[{}, {}, {}]", f(p.x), f(p.y), f(p.z))
}

fn eval(e: &LawExpr, p: DVec3, slots: &mut Vec<LawVal>) -> LawVal {
    match e {
        LawExpr::Undef => LawVal::Undef,
        LawExpr::Bool(b) => LawVal::Bool(*b),
        LawExpr::Num(x) => LawVal::Num(*x),
        LawExpr::Point => LawVal::List(vec![LawVal::Num(p.x), LawVal::Num(p.y), LawVal::Num(p.z)]),
        LawExpr::Slot(i) => slots.get(*i as usize).cloned().unwrap_or(LawVal::Undef),
        LawExpr::List(l) => LawVal::List(l.iter().map(|x| eval(x, p, slots)).collect()),
        LawExpr::Neg(x) => {
            fn neg(v: LawVal) -> LawVal {
                match v {
                    LawVal::Num(x) => LawVal::Num(-x),
                    LawVal::List(l) => LawVal::List(l.into_iter().map(neg).collect()),
                    _ => LawVal::Undef,
                }
            }
            neg(eval(x, p, slots))
        }
        LawExpr::Not(x) => LawVal::Bool(!eval(x, p, slots).truthy()),
        LawExpr::Bin(op, a, b) => {
            let a = eval(a, p, slots);
            let b = eval(b, p, slots);
            binop(*op, &a, &b)
        }
        LawExpr::And(a, b) => LawVal::Bool(eval(a, p, slots).truthy() && eval(b, p, slots).truthy()),
        LawExpr::Or(a, b) => LawVal::Bool(eval(a, p, slots).truthy() || eval(b, p, slots).truthy()),
        LawExpr::Cond(c, a, b) => {
            if eval(c, p, slots).truthy() {
                eval(a, p, slots)
            } else {
                eval(b, p, slots)
            }
        }
        LawExpr::Index(base, i) => {
            let b = eval(base, p, slots);
            let i = eval(i, p, slots);
            match (b, i.num()) {
                (LawVal::List(l), Some(k)) if k >= 0.0 && k.is_finite() => {
                    l.get(k.floor() as usize).cloned().unwrap_or(LawVal::Undef)
                }
                _ => LawVal::Undef,
            }
        }
        LawExpr::Call(f, args) => {
            let a: Vec<LawVal> = args.iter().map(|x| eval(x, p, slots)).collect();
            call(*f, &a)
        }
        LawExpr::Let(i, v, body) => {
            let v = eval(v, p, slots);
            if let Some(s) = slots.get_mut(*i as usize) {
                *s = v;
            }
            eval(body, p, slots)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(e: LawExpr) -> Box<LawExpr> {
        Box::new(e)
    }

    #[test]
    fn step_law() {
        // p.z < 10 ? 2 : 4
        let law = Law {
            expr: LawExpr::Cond(
                b(LawExpr::Bin(LawOp::Lt, b(LawExpr::Index(b(LawExpr::Point), b(LawExpr::Num(2.0)))), b(LawExpr::Num(10.0)))),
                b(LawExpr::Num(2.0)),
                b(LawExpr::Num(4.0)),
            ),
            slots: 0,
            text: String::new(),
        };
        assert_eq!(law.eval(DVec3::new(0.0, 0.0, 5.0)), Ok(2.0));
        assert_eq!(law.eval(DVec3::new(0.0, 0.0, 15.0)), Ok(4.0));
    }

    #[test]
    fn let_lookup_and_errors() {
        // let(t = norm(p)) lookup(t, [[0, 1], [10, 3]])
        let table = LawExpr::List(vec![
            LawExpr::List(vec![LawExpr::Num(0.0), LawExpr::Num(1.0)]),
            LawExpr::List(vec![LawExpr::Num(10.0), LawExpr::Num(3.0)]),
        ]);
        let law = Law {
            expr: LawExpr::Let(
                0,
                b(LawExpr::Call(LawFn::Norm, vec![LawExpr::Point])),
                b(LawExpr::Call(LawFn::Lookup, vec![LawExpr::Slot(0), table])),
            ),
            slots: 1,
            text: String::new(),
        };
        assert_eq!(law.eval(DVec3::new(3.0, 4.0, 0.0)), Ok(2.0));
        assert_eq!(law.eval(DVec3::new(30.0, 40.0, 0.0)), Ok(3.0));
        let bad = Law { expr: LawExpr::Point, slots: 0, text: String::new() };
        assert!(bad.eval(DVec3::ZERO).unwrap_err().contains("keine Zahl"));
        assert_eq!(cos_deg(90.0), 0.0);
        assert_eq!(sin_deg(-90.0), -1.0);
    }
}
