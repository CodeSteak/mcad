//! Operators with OpenSCAD semantics. Results are always [`Origin::Computed`].
//! An `Err` carries a warning; the result is then `undef`.

use cad_i18n::{tr, trs};
use std::cmp::Ordering;
use std::rc::Rc;

use cad_syntax::ast::{BinOp, UnOp};

use crate::value::{Val, Value, values_eq};

type Res<'a> = Result<Val<'a>, String>;

fn undefined_op(op: &str, a: &Val, b: &Val) -> String {
    tr!("undefinierte Operation ({} {op} {})", "undefined operation ({} {op} {})", a.type_de(), b.type_de())
}

fn op_sym(op: BinOp) -> &'static str {
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

fn list_of<'a>(v: Vec<Val<'a>>) -> Val<'a> {
    Val::new(Value::List(Rc::new(v)))
}

/// Elementwise `+`/`-` (vectors truncated to the shorter one).
fn addsub<'a>(a: &Val<'a>, b: &Val<'a>, sub: bool) -> Option<Val<'a>> {
    if let (Some(x), Some(y)) = (a.as_num(), b.as_num()) {
        return Some(Val::num(if sub { x - y } else { x + y }));
    }
    match (&a.v, &b.v) {
        (Value::List(x), Value::List(y)) => {
            Some(list_of(x.iter().zip(y.iter()).map(|(p, q)| addsub(p, q, sub).unwrap_or_else(Val::undef)).collect()))
        }
        _ => None,
    }
}

fn scale_list<'a>(l: &[Val<'a>], k: f64, div: bool, left: bool) -> Val<'a> {
    list_of(
        l.iter()
            .map(|e| match (&e.v, e.as_num()) {
                (_, Some(n)) => Val::num(if div { if left { k / n } else { n / k } } else { n * k }),
                (Value::List(inner), _) => scale_list(inner, k, div, left),
                _ => Val::undef(),
            })
            .collect(),
    )
}

fn nums(l: &[Val]) -> Option<Vec<f64>> {
    l.iter().map(|v| v.as_num()).collect()
}

fn matrix(l: &[Val]) -> Option<Vec<Vec<f64>>> {
    if l.is_empty() {
        return None;
    }
    l.iter().map(|r| r.as_list().and_then(nums)).collect()
}

fn mul<'a>(a: &Val<'a>, b: &Val<'a>) -> Res<'a> {
    if let (Some(x), Some(y)) = (a.as_num(), b.as_num()) {
        return Ok(Val::num(x * y));
    }
    match (&a.v, &b.v) {
        (Value::List(l), _) if b.as_num().is_some() => Ok(scale_list(l, b.as_num().unwrap(), false, false)),
        (_, Value::List(l)) if a.as_num().is_some() => Ok(scale_list(l, a.as_num().unwrap(), false, false)),
        (Value::List(x), Value::List(y)) => {
            if x.is_empty() || y.is_empty() {
                return Err(trs!(
                    "Multiplikation mit leerem Vektor ist undefiniert",
                    "multiplication with an empty vector is undefined"
                )
                .into());
            }
            if let (Some(u), Some(v)) = (nums(x), nums(y)) {
                if u.len() != v.len() {
                    return Ok(Val::undef());
                }
                return Ok(Val::num(u.iter().zip(&v).map(|(p, q)| p * q).sum()));
            }
            if let (Some(m), Some(v)) = (matrix(x), nums(y)) {
                if m.iter().any(|r| r.len() != v.len()) {
                    return Err(tr!(
                        "Matrix * Vektor: Spaltenzahl der Matrix ≠ Länge des Vektors ({} ≠ {})",
                        "matrix * vector: column count of the matrix ≠ length of the vector ({} ≠ {})",
                        m[0].len(),
                        v.len()
                    ));
                }
                return Ok(list_of(m.iter().map(|r| Val::num(r.iter().zip(&v).map(|(p, q)| p * q).sum())).collect()));
            }
            if let (Some(u), Some(m)) = (nums(x), matrix(y)) {
                return Ok(vec_mat(&u, &m)
                    .map(|r| list_of(r.into_iter().map(Val::num).collect()))
                    .unwrap_or_else(Val::undef));
            }
            if let (Some(a), Some(b)) = (matrix(x), matrix(y)) {
                let rows: Option<Vec<Vec<f64>>> = a.iter().map(|r| vec_mat(r, &b)).collect();
                return Ok(match rows {
                    Some(rows) => {
                        list_of(rows.into_iter().map(|r| list_of(r.into_iter().map(Val::num).collect())).collect())
                    }
                    None => Val::undef(),
                });
            }
            Ok(Val::undef())
        }
        _ => Err(undefined_op("*", a, b)),
    }
}

fn vec_mat(u: &[f64], m: &[Vec<f64>]) -> Option<Vec<f64>> {
    if u.len() != m.len() {
        return None;
    }
    let cols = m[0].len();
    if m.iter().any(|r| r.len() != cols) {
        return None;
    }
    Some((0..cols).map(|j| u.iter().zip(m).map(|(a, r)| a * r[j]).sum()).collect())
}

fn div<'a>(a: &Val<'a>, b: &Val<'a>) -> Res<'a> {
    if let (Some(x), Some(y)) = (a.as_num(), b.as_num()) {
        return Ok(Val::num(x / y));
    }
    match (&a.v, &b.v) {
        (Value::List(l), _) if b.as_num().is_some() => Ok(scale_list(l, b.as_num().unwrap(), true, false)),
        (_, Value::List(l)) if a.as_num().is_some() => Ok(scale_list(l, a.as_num().unwrap(), true, true)),
        _ => Err(undefined_op("/", a, b)),
    }
}

/// Ordering for `<`, `>`, …: numbers, strings, bools, and vectors lexicographically.
pub(crate) fn compare(a: &Val, b: &Val) -> Option<Option<Ordering>> {
    if let (Some(x), Some(y)) = (a.as_num(), b.as_num()) {
        return Some(x.partial_cmp(&y));
    }
    match (&a.v, &b.v) {
        (Value::Str(x), Value::Str(y)) => Some(Some(x.cmp(y))),
        (Value::Bool(x), Value::Bool(y)) => Some(Some(x.cmp(y))),
        (Value::List(x), Value::List(y)) => {
            for (p, q) in x.iter().zip(y.iter()) {
                match compare(p, q)? {
                    Some(Ordering::Equal) => continue,
                    o => return Some(o),
                }
            }
            Some(Some(x.len().cmp(&y.len())))
        }
        _ => None,
    }
}

#[inline(never)]
pub(crate) fn binop<'a>(op: BinOp, a: &Val<'a>, b: &Val<'a>) -> Res<'a> {
    let r = match op {
        BinOp::Add | BinOp::Sub => {
            return addsub(a, b, op == BinOp::Sub).ok_or_else(|| undefined_op(op_sym(op), a, b));
        }
        BinOp::Mul => return mul(a, b),
        BinOp::Div => return div(a, b),
        BinOp::Mod => match (a.as_num(), b.as_num()) {
            (Some(x), Some(y)) => Val::num(x % y),
            _ => return Err(undefined_op("%", a, b)),
        },
        BinOp::Pow => match (a.as_num(), b.as_num()) {
            (Some(x), Some(y)) => Val::num(x.powf(y)),
            _ => return Err(undefined_op("^", a, b)),
        },
        BinOp::Eq => Val::bool(values_eq(a, b)),
        BinOp::Ne => Val::bool(!values_eq(a, b)),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => match compare(a, b) {
            None => return Err(undefined_op(op_sym(op), a, b)),
            Some(None) => Val::bool(false),
            Some(Some(o)) => Val::bool(match op {
                BinOp::Lt => o == Ordering::Less,
                BinOp::Le => o != Ordering::Greater,
                BinOp::Gt => o == Ordering::Greater,
                _ => o != Ordering::Less,
            }),
        },
        BinOp::And => Val::bool(a.truthy() && b.truthy()),
        BinOp::Or => Val::bool(a.truthy() || b.truthy()),
    };
    Ok(r)
}

fn neg<'a>(a: &Val<'a>) -> Option<Val<'a>> {
    if let Some(x) = a.as_num() {
        return Some(Val::num(-x));
    }
    match &a.v {
        Value::List(l) => Some(list_of(l.iter().map(|e| neg(e).unwrap_or_else(Val::undef)).collect())),
        _ => None,
    }
}

#[inline(never)]
pub(crate) fn unop<'a>(op: UnOp, a: &Val<'a>) -> Res<'a> {
    match op {
        UnOp::Not => Ok(Val::bool(!a.truthy())),
        UnOp::Neg => {
            neg(a).ok_or_else(|| tr!("undefinierte Operation (-{})", "undefined operation (-{})", a.type_de()))
        }
        // OpenSCAD: unary `+` returns its operand unchanged, whatever its type.
        UnOp::Plus => Ok(match a.as_num() {
            Some(x) => Val::num(x),
            None => Val::new(a.v.clone()),
        }),
    }
}
