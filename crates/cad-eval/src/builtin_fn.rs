//! Built-in functions (OpenSCAD set plus `dir()`).

use cad_syntax::ast::Expr;

use crate::env::Env;
use crate::eval::{Args, Ev, R};
use crate::math::{cos_deg, sin_deg, tan_deg};
use crate::value::{Val, Value, to_str, values_eq};

fn nums(v: &Val) -> Option<Vec<f64>> {
    v.as_list()?.iter().map(|x| x.as_num()).collect()
}

impl<'a> Ev<'a> {
    /// Calls a built-in function; `None` if `name` is not one.
    pub(crate) fn builtin_fn(&mut self, name: &str, a: &Args<'a>, call: &'a Expr, _env: &Env<'a>) -> R<Option<Val<'a>>> {
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
            "sign" => num1(|x| if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 }),
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
            "min" | "max" => {
                let list: Option<Vec<f64>> = if pos.len() == 1 {
                    match pos[0].as_num() {
                        Some(n) => Some(vec![n]),
                        None => nums(pos[0]),
                    }
                } else {
                    pos.iter().map(|v| v.as_num()).collect()
                };
                match list {
                    Some(l) if !l.is_empty() => {
                        let r = if name == "min" {
                            l.iter().copied().fold(f64::INFINITY, f64::min)
                        } else {
                            l.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                        };
                        Val::num(r)
                    }
                    _ => {
                        self.warn(span, format!("{name}() parameter could not be converted"));
                        Val::undef()
                    }
                }
            }
            "norm" => match pos.first().and_then(|v| nums(v)) {
                Some(l) => Val::num(l.iter().map(|x| x * x).sum::<f64>().sqrt()),
                None => {
                    self.warn(span, "incorrect arguments to norm()");
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
                    self.warn(span, "invalid arguments to cross()");
                    Val::undef()
                }
            },
            "len" => match pos.first().map(|v| &v.v) {
                Some(Value::List(l)) => Val::num(l.len() as f64),
                Some(Value::Str(s)) => Val::num(s.chars().count() as f64),
                _ => {
                    self.warn(span, "len() parameter could not be converted");
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
            "lookup" => lookup(arg(0, "key"), arg(1, "table")),
            "str" => Val::str(&pos.iter().map(|v| to_str(v)).collect::<String>()),
            "chr" => {
                let mut s = String::new();
                fn add(v: &Val, s: &mut String) {
                    match &v.v {
                        Value::Num(n) => {
                            if let Some(c) = (*n >= 0.0 && n.fract() == 0.0).then(|| char::from_u32(*n as u32)).flatten()
                                && c != '\0'
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
                Some(Value::Str(s)) => {
                    let mut it = s.chars();
                    match (it.next(), it.next()) {
                        (Some(c), None) => Val::num(c as u32 as f64),
                        _ => Val::undef(),
                    }
                }
                Some(_) => {
                    self.warn(span, "ord() argument is not a string");
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
                    self.warn(span, "rands() needs min_value, max_value and value_count");
                    return Ok(Some(Val::undef()));
                };
                let n = n.max(0.0).min(self.opts.max_list_len as f64) as usize;
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
                    self.error(span, "dir() expects an anchor");
                    Val::undef()
                }
            },
            _ => return Ok(None),
        };
        Ok(Some(v))
    }

    fn search(&mut self, a: &Args<'a>, span: cad_syntax::Span) -> Val<'a> {
        let (Some(m), Some(hay)) = (a.get(0, "match_value"), a.get(1, "string_or_vector")) else {
            self.warn(span, "search() needs match_value and string_or_vector");
            return Val::undef();
        };
        let nret = a.get(2, "num_returns_per_match").and_then(|v| v.val.as_num()).unwrap_or(1.0) as usize;
        let col = a.get(3, "index_col_num").and_then(|v| v.val.as_num()).unwrap_or(0.0) as usize;
        let (m, hay) = (m.val.clone(), hay.val.clone());
        // Candidate key of each haystack entry.
        let keys: Vec<Option<Val<'a>>> = match &hay.v {
            Value::Str(s) => s.chars().map(|c| Some(Val::str(&c.to_string()))).collect(),
            Value::List(l) => l
                .iter()
                .map(|e| match &e.v {
                    Value::List(r) => r.get(col).cloned(),
                    _ if col == 0 => Some(e.clone()),
                    _ => None,
                })
                .collect(),
            _ => return Val::undef(),
        };
        let hay_is_str = matches!(hay.v, Value::Str(_));
        let find = |key: &Val<'a>, max: usize| -> Vec<Val<'a>> {
            let mut out = Vec::new();
            for (i, k) in keys.iter().enumerate() {
                if let Some(k) = k
                    && values_eq(k, key)
                {
                    out.push(Val::num(i as f64));
                    if max != 0 && out.len() >= max {
                        break;
                    }
                }
            }
            out
        };
        match &m.v {
            Value::Num(_) | Value::Bool(_) | Value::Undef => Val::list(find(&m, nret)),
            Value::Str(s) => {
                if !hay_is_str && hay.as_list().is_some_and(|l| l.iter().any(|e| e.as_list().is_none())) {
                    self.warn(span, "invalid entry in search vector: entries must be lists");
                    return Val::list(vec![]);
                }
                let mut out = Vec::new();
                for c in s.chars() {
                    let key = Val::str(&c.to_string());
                    let r = find(&key, nret);
                    if nret == 1 {
                        match r.into_iter().next() {
                            Some(i) => out.push(i),
                            None => self.warn(span, format!("search term not found: \"{c}\"")),
                        }
                    } else {
                        out.push(Val::list(r));
                    }
                }
                Val::list(out)
            }
            Value::List(l) => {
                let mut out = Vec::new();
                for key in l.iter() {
                    let r = find(key, nret);
                    if nret == 1 {
                        out.push(r.into_iter().next().unwrap_or_else(|| Val::list(vec![])));
                    } else {
                        out.push(Val::list(r));
                    }
                }
                Val::list(out)
            }
            _ => Val::undef(),
        }
    }
}

/// OpenSCAD `lookup()` (linear interpolation, clamped, unsorted tables allowed).
fn lookup<'a>(key: Option<&Val<'a>>, table: Option<&Val<'a>>) -> Val<'a> {
    let (Some(p), Some(t)) = (key.and_then(|k| k.as_num()), table.and_then(|t| t.as_list())) else {
        return Val::undef();
    };
    let pair = |v: &Val| -> Option<(f64, f64)> {
        let l = v.as_list()?;
        if l.len() < 2 {
            return None;
        }
        Some((l[0].as_num()?, l[1].as_num()?))
    };
    let Some(first) = t.first().and_then(pair) else {
        return Val::undef();
    };
    let (mut low, mut high) = (first, first);
    for e in &t[1..] {
        let Some((tp, tv)) = pair(e) else { return Val::undef() };
        if tp <= p && (tp > low.0 || low.0 > p) {
            low = (tp, tv);
        }
        if tp >= p && (tp < high.0 || high.0 < p) {
            high = (tp, tv);
        }
    }
    if p <= low.0 {
        return Val::num(low.1);
    }
    if p >= high.0 {
        return Val::num(high.1);
    }
    let f = (p - low.0) / (high.0 - low.0);
    Val::num(high.1 * f + low.1 * (1.0 - f))
}
