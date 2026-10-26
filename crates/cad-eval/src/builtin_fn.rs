//! Built-in functions (OpenSCAD set plus `dir()`).

use cad_i18n::{tr, trs};
use std::cmp::Ordering;

use cad_syntax::ast::Expr;

use crate::env::Env;
use crate::eval::{Args, Ev, R};
use crate::math::{cos_deg, sin_deg, tan_deg};
use crate::ops::compare;
use crate::value::{Val, Value, to_display, to_str, values_eq};

fn nums(v: &Val) -> Option<Vec<f64>> {
    v.as_list()?.iter().map(|x| x.as_num()).collect()
}

impl<'a> Ev<'a> {
    /// Calls a built-in function; `None` if `name` is not one.
    #[inline(never)]
    pub(crate) fn builtin_fn(
        &mut self,
        name: &str,
        a: &Args<'a>,
        call: &'a Expr,
        _env: &Env<'a>,
    ) -> R<Option<Val<'a>>> {
        let span = call.span;
        let pos: Vec<&Val<'a>> = a.positional().map(|x| &x.val).collect();
        let arg = |i: usize, n: &str| a.get(i, n).map(|x| &x.val);
        let num1 = |f: fn(f64) -> f64| -> Val<'a> {
            match pos.first().and_then(|v| v.as_num()) {
                Some(x) => Val::num(f(x)),
                None => Val::undef(),
            }
        };
        let v = match name {
            "abs" => num1(f64::abs),
            "sign" => num1(|x| {
                if x > 0.0 {
                    1.0
                } else if x < 0.0 {
                    -1.0
                } else {
                    0.0
                }
            }),
            "sin" => num1(sin_deg),
            "cos" => num1(cos_deg),
            "tan" => num1(tan_deg),
            "asin" => num1(|x| x.asin().to_degrees()),
            "acos" => num1(|x| x.acos().to_degrees()),
            "atan" => num1(|x| x.atan().to_degrees()),
            "floor" => num1(f64::floor),
            "ceil" => num1(f64::ceil),
            "round" => num1(f64::round),
            "ln" => num1(f64::ln),
            "exp" => num1(f64::exp),
            "sqrt" => num1(f64::sqrt),
            "log" => match (pos.first().and_then(|v| v.as_num()), pos.get(1).and_then(|v| v.as_num())) {
                (Some(b), Some(x)) => Val::num(x.ln() / b.ln()),
                (Some(x), None) => Val::num(x.log10()),
                _ => Val::undef(),
            },
            "atan2" => match (pos.first().and_then(|v| v.as_num()), pos.get(1).and_then(|v| v.as_num())) {
                (Some(y), Some(x)) => Val::num(y.atan2(x).to_degrees()),
                _ => Val::undef(),
            },
            "pow" => match (pos.first().and_then(|v| v.as_num()), pos.get(1).and_then(|v| v.as_num())) {
                (Some(b), Some(e)) => Val::num(b.powf(e)),
                _ => Val::undef(),
            },
            "min" | "max" => match min_max(&pos, name == "max") {
                Some(v) => v,
                None => {
                    self.warn(
                        span,
                        tr!(
                            "{name}(): Argumente sind weder Zahlen noch ein Vektor",
                            "{name}(): arguments are neither numbers nor a vector"
                        ),
                    );
                    Val::undef()
                }
            },
            "norm" => match pos.first().and_then(|v| nums(v)) {
                Some(l) => Val::num(l.iter().map(|x| x * x).sum::<f64>().sqrt()),
                None => {
                    self.warn(
                        span,
                        trs!("norm(): Argument ist kein Zahlenvektor", "norm(): argument is not a number vector"),
                    );
                    Val::undef()
                }
            },
            "cross" => match (pos.first().and_then(|v| nums(v)), pos.get(1).and_then(|v| nums(v))) {
                (Some(x), Some(y)) if x.len() == 3 && y.len() == 3 => Val::list(vec![
                    Val::num(x[1] * y[2] - x[2] * y[1]),
                    Val::num(x[2] * y[0] - x[0] * y[2]),
                    Val::num(x[0] * y[1] - x[1] * y[0]),
                ]),
                (Some(x), Some(y)) if x.len() == 2 && y.len() == 2 => Val::num(x[0] * y[1] - x[1] * y[0]),
                _ => {
                    self.warn(
                        span,
                        trs!(
                            "cross(): braucht zwei Zahlenvektoren der Länge 2 oder 3",
                            "cross(): needs two number vectors of length 2 or 3"
                        ),
                    );
                    Val::undef()
                }
            },
            "len" => match pos.first().map(|v| &v.v) {
                Some(Value::List(l)) => Val::num(l.len() as f64),
                Some(Value::Str(s)) => Val::num(s.chars().count() as f64),
                _ => {
                    self.warn(
                        span,
                        trs!(
                            "len(): Argument ist weder Vektor noch String",
                            "len(): argument is neither vector nor string"
                        ),
                    );
                    Val::undef()
                }
            },
            "concat" => {
                let mut out = Vec::new();
                for v in &pos {
                    match &v.v {
                        Value::List(l) => out.extend(l.iter().cloned()),
                        _ => out.push((*v).clone()),
                    }
                }
                Val::list(out)
            }
            "lookup" => match lookup(arg(0, "key"), arg(1, "table")) {
                Ok(v) => v,
                Err(msg) => {
                    self.warn(span, msg);
                    Val::undef()
                }
            },
            "str" => Val::str(&pos.iter().map(|v| to_str(v)).collect::<String>()),
            "chr" => {
                let mut s = String::new();
                fn add(v: &Val, s: &mut String) {
                    match &v.v {
                        // OpenSCAD truncates (`chr(65.5)` is "A"); 0 and invalid code points
                        // give nothing.
                        Value::Num(n) => {
                            let t = n.trunc();
                            if (1.0..=u32::MAX as f64).contains(&t)
                                && let Some(c) = char::from_u32(t as u32)
                            {
                                s.push(c);
                            }
                        }
                        Value::List(l) => l.iter().for_each(|x| add(x, s)),
                        Value::Range(r) => {
                            let n = r.count().min(1e6) as usize;
                            (0..n).for_each(|i| add(&Val::num(r.at(i)), s));
                        }
                        _ => {}
                    }
                }
                for v in &pos {
                    add(v, &mut s);
                }
                Val::str(&s)
            }
            "ord" => match pos.first().map(|v| &v.v) {
                Some(Value::Str(s)) => match s.chars().next() {
                    Some(c) => Val::num(c as u32 as f64),
                    None => Val::undef(),
                },
                Some(_) => {
                    self.warn(span, trs!("ord(): Argument ist kein String", "ord(): argument is not a string"));
                    Val::undef()
                }
                None => Val::undef(),
            },
            "search" => self.search(a, span),
            "rands" => {
                let (Some(lo), Some(hi), Some(n)) = (
                    arg(0, "min_value").and_then(|v| v.as_num()),
                    arg(1, "max_value").and_then(|v| v.as_num()),
                    arg(2, "value_count").and_then(|v| v.as_num()),
                ) else {
                    self.warn(
                        span,
                        trs!(
                            "rands() braucht `min_value`, `max_value` und `value_count`",
                            "rands() needs `min_value`, `max_value` and `value_count`"
                        ),
                    );
                    return Ok(Some(Val::undef()));
                };
                // OpenSCAD takes |value_count|, truncated.
                let n = n.abs().min(self.opts.max_list_len as f64) as usize;
                let seed = arg(3, "seed_value").and_then(|v| v.as_num());
                let saved = self.rng;
                if let Some(s) = seed {
                    self.rng = s.to_bits() ^ 0x5eed_5eed;
                }
                let out = (0..n).map(|_| Val::num(lo + (hi - lo) * self.rand())).collect();
                if seed.is_some() {
                    self.rng = saved;
                }
                Val::list(out)
            }
            "is_undef" => Val::bool(pos.first().is_none_or(|v| v.is_undef())),
            "is_bool" => Val::bool(matches!(pos.first().map(|v| &v.v), Some(Value::Bool(_)))),
            "is_num" => Val::bool(pos.first().and_then(|v| v.as_num()).is_some_and(|n| !n.is_nan())),
            "is_string" => Val::bool(matches!(pos.first().map(|v| &v.v), Some(Value::Str(_)))),
            "is_list" => Val::bool(matches!(pos.first().map(|v| &v.v), Some(Value::List(_)))),
            "is_function" => Val::bool(matches!(pos.first().map(|v| &v.v), Some(Value::Func(_)))),
            "version" => Val::list(vec![Val::num(2021.0), Val::num(1.0), Val::num(0.0)]),
            "version_num" => Val::num(20210100.0),
            "dir" => match pos.first() {
                Some(v) => {
                    let v = (*v).clone();
                    self.anchor_dir(&v, span)
                }
                None => {
                    self.error(span, trs!("dir() erwartet einen Anker", "dir() expects an anchor"));
                    Val::undef()
                }
            },
            _ => return Ok(None),
        };
        Ok(Some(v))
    }

    /// OpenSCAD 2021 `search()`, including its quirks: a string searched in a list
    /// compares each character with the first character of the entry's column (as
    /// text), a list of match values searched in a string finds nothing, and an entry
    /// too short for `index_col_num` ends a string search with an empty result.
    fn search(&mut self, a: &Args<'a>, span: cad_syntax::Span) -> Val<'a> {
        let (Some(m), Some(hay)) = (a.get(0, "match_value"), a.get(1, "string_or_vector")) else {
            self.warn(
                span,
                trs!(
                    "search() braucht `match_value` und `string_or_vector`",
                    "search() needs `match_value` and `string_or_vector`"
                ),
            );
            return Val::undef();
        };
        // Given but not a number counts as 0 (OpenSCAD's `toDouble()`).
        let count_arg = |i: usize, n: &str, dflt: f64| match a.get(i, n) {
            None => dflt,
            Some(v) => v.val.as_num().filter(|x| x.is_finite()).unwrap_or(0.0).max(0.0),
        };
        let nret = count_arg(2, "num_returns_per_match", 1.0) as usize;
        let col = count_arg(3, "index_col_num", 0.0) as usize;
        let (m, hay) = (m.val.clone(), hay.val.clone());
        let table: &[Val<'a>] = hay.as_list().unwrap_or(&[]);
        let full = |found: &[Val<'a>]| nret != 0 && found.len() >= nret;
        // Entry `e` matches the number or list element `v`.
        let matches = |v: &Val<'a>, e: &Val<'a>| {
            (col == 0 && values_eq(v, e)) || e.as_list().and_then(|l| l.get(col)).is_some_and(|x| values_eq(v, x))
        };
        let mut out = Vec::new();
        match &m.v {
            Value::Num(_) | Value::PlaneRef { .. } => {
                for (j, e) in table.iter().enumerate() {
                    if matches(&m, e) {
                        out.push(Val::num(j as f64));
                        if full(&out) {
                            break;
                        }
                    }
                }
            }
            Value::Str(s) => {
                for c in s.chars() {
                    let mut found = Vec::new();
                    if let Value::Str(t) = &hay.v {
                        for (j, tc) in t.chars().enumerate() {
                            if tc == c {
                                found.push(Val::num(j as f64));
                                if full(&found) {
                                    break;
                                }
                            }
                        }
                        if found.is_empty() {
                            self.warn(span, tr!("search(): \"{c}\" nicht gefunden", "search(): \"{c}\" not found"));
                        }
                    } else {
                        for (j, e) in table.iter().enumerate() {
                            let Some(x) = e.as_list().and_then(|l| l.get(col)) else {
                                let (txt, k) = (to_display(e), col + 1);
                                self.warn(
                                    span,
                                    tr!(
                                        "search(): Eintrag {j} ({txt}) hat weniger als {k} Werte",
                                        "search(): entry {j} ({txt}) has fewer than {k} values"
                                    ),
                                );
                                return Val::list(vec![]);
                            };
                            if to_str(x).starts_with(c) {
                                found.push(Val::num(j as f64));
                                if full(&found) {
                                    break;
                                }
                            }
                        }
                    }
                    if nret == 1 {
                        out.extend(found);
                    } else {
                        out.push(Val::list(found));
                    }
                }
            }
            Value::List(l) => {
                for key in l.iter() {
                    let mut found = Vec::new();
                    for (j, e) in table.iter().enumerate() {
                        if matches(key, e) {
                            found.push(Val::num(j as f64));
                            if full(&found) {
                                break;
                            }
                        }
                    }
                    if nret == 1 && !found.is_empty() {
                        out.extend(found);
                    } else {
                        out.push(Val::list(found));
                    }
                }
            }
            _ => {
                let t = m.type_name();
                self.warn(
                    span,
                    tr!(
                        "search(): Suchwert vom Typ {t} nicht möglich",
                        "search(): search value of type {t} not possible"
                    ),
                );
                return Val::undef();
            }
        }
        Val::list(out)
    }
}

/// OpenSCAD `min()` / `max()`: numbers only (anything else gives `undef`), or the
/// smallest / largest element of a single non-empty vector by `<` / `>` (elements of
/// different types never compare, so they are passed over). `None` = `undef`.
fn min_max<'a>(pos: &[&Val<'a>], max: bool) -> Option<Val<'a>> {
    let first = pos.first()?;
    if pos.len() == 1
        && let Some(l) = first.as_list()
        && !l.is_empty()
    {
        let want = if max { Ordering::Greater } else { Ordering::Less };
        let mut best = &l[0];
        for v in &l[1..] {
            if compare(v, best) == Some(Some(want)) {
                best = v;
            }
        }
        return Some(Val::new(best.v.clone()));
    }
    let mut val = first.as_num()?;
    for v in &pos[1..] {
        let x = v.as_num()?;
        if (max && x > val) || (!max && x < val) {
            val = x;
        }
    }
    Some(Val::num(val))
}

/// `[p, v]` with exactly two numbers (OpenSCAD `getVec2`).
fn vec2(v: &Val) -> Option<(f64, f64)> {
    match v.as_list()? {
        [p, x] => Some((p.as_num()?, x.as_num()?)),
        _ => None,
    }
}

/// OpenSCAD `lookup()`: linear interpolation, clamped, unsorted tables allowed. The
/// key must be finite and the first entry a `[p, v]` pair; later invalid entries are
/// skipped.
fn lookup<'a>(key: Option<&Val<'a>>, table: Option<&Val<'a>>) -> Result<Val<'a>, String> {
    let Some(p) = key.and_then(|k| k.as_num()).filter(|p| p.is_finite()) else {
        let k = key.map_or("undef".into(), to_display);
        return Err(tr!("lookup(): Schlüssel {k} ist keine Zahl", "lookup(): key {k} is not a number"));
    };
    let Some(t) = table.and_then(|t| t.as_list()) else { return Ok(Val::undef()) };
    let Some(first) = t.first().and_then(vec2) else {
        return Ok(Val::undef());
    };
    let (mut low, mut high) = (first, first);
    for (tp, tv) in t[1..].iter().filter_map(vec2) {
        if tp <= p && (tp > low.0 || low.0 > p) {
            low = (tp, tv);
        }
        if tp >= p && (tp < high.0 || high.0 < p) {
            high = (tp, tv);
        }
    }
    // Outside the table one of the two stays at the first entry (as in OpenSCAD).
    if p <= low.0 {
        return Ok(Val::num(high.1));
    }
    if p >= high.0 {
        return Ok(Val::num(low.1));
    }
    let f = (p - low.0) / (high.0 - low.0);
    Ok(Val::num(high.1 * f + low.1 * (1.0 - f)))
}
