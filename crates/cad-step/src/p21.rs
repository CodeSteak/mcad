//! Minimal reader for ISO 10303-21 exchange files (as written by [`crate::write`]): checks the
//! syntax and every reference, and gives access to entity parameters. Tests and diagnostics
//! use it; it is not a general STEP importer.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Ref(u32),
    Num(f64),
    Str(String),
    Enum(String),
    List(Vec<Value>),
    /// `$`
    Unset,
    /// `*`
    Derived,
    /// Typed parameter, e.g. `LENGTH_MEASURE(1.E-6)`.
    Typed(String, Vec<Value>),
}

impl Value {
    pub fn as_ref_id(&self) -> Option<u32> {
        if let Value::Ref(r) = self { Some(*r) } else { None }
    }
    pub fn as_num(&self) -> Option<f64> {
        if let Value::Num(x) = self { Some(*x) } else { None }
    }
    pub fn as_list(&self) -> Option<&[Value]> {
        if let Value::List(l) = self { Some(l) } else { None }
    }
    pub fn as_enum(&self) -> Option<&str> {
        if let Value::Enum(e) = self { Some(e) } else { None }
    }
}

/// One entity instance: simple (one part) or complex (several, alphabetical).
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: u32,
    pub parts: Vec<(String, Vec<Value>)>,
}

impl Entity {
    /// Type name; complex instances have their part names joined with `+`.
    pub fn ty(&self) -> String {
        self.parts.iter().map(|p| p.0.as_str()).collect::<Vec<_>>().join("+")
    }
    /// Parameters of a simple instance (first part).
    pub fn args(&self) -> &[Value] {
        &self.parts[0].1
    }
    pub fn part(&self, name: &str) -> Option<&[Value]> {
        self.parts.iter().find(|p| p.0 == name).map(|p| p.1.as_slice())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Model {
    pub schema: String,
    pub entities: BTreeMap<u32, Entity>,
}

impl Model {
    pub fn get(&self, id: u32) -> Option<&Entity> {
        self.entities.get(&id)
    }
    /// Entities whose type (or one of whose parts) is `ty`.
    pub fn of_type<'a>(&'a self, ty: &'a str) -> impl Iterator<Item = &'a Entity> + 'a {
        self.entities.values().filter(move |e| e.parts.iter().any(|p| p.0 == ty))
    }
    pub fn count(&self, ty: &str) -> usize {
        self.of_type(ty).count()
    }
    /// Coordinates of a `CARTESIAN_POINT` or `DIRECTION`.
    pub fn coords(&self, id: u32) -> Option<[f64; 3]> {
        let e = self.get(id)?;
        let l = e.args().get(1)?.as_list()?;
        Some([l.first()?.as_num()?, l.get(1)?.as_num()?, l.get(2)?.as_num()?])
    }
}

struct Lx<'a> {
    s: &'a [u8],
    i: usize,
}

impl Lx<'_> {
    fn ws(&mut self) {
        loop {
            while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
            if self.s[self.i..].starts_with(b"/*") {
                match find(&self.s[self.i..], b"*/") {
                    Some(k) => self.i += k + 2,
                    None => self.i = self.s.len(),
                }
            } else {
                break;
            }
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }
    fn eat(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at byte {}", c as char, self.i))
        }
    }
    fn keyword(&mut self) -> Result<String, String> {
        self.ws();
        let st = self.i;
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_' || self.s[self.i] == b'-')
        {
            self.i += 1;
        }
        if st == self.i {
            return Err(format!("expected keyword at byte {st}"));
        }
        Ok(String::from_utf8_lossy(&self.s[st..self.i]).into_owned())
    }
    fn uint(&mut self) -> Result<u32, String> {
        let st = self.i;
        while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[st..self.i]).unwrap().parse().map_err(|_| format!("bad id at byte {st}"))
    }
    fn value(&mut self) -> Result<Value, String> {
        match self.peek().ok_or("unexpected end")? {
            b'#' => {
                self.i += 1;
                Ok(Value::Ref(self.uint()?))
            }
            b'\'' => {
                self.i += 1;
                let mut out = Vec::new();
                loop {
                    let c = *self.s.get(self.i).ok_or("unterminated string")?;
                    self.i += 1;
                    if c == b'\'' {
                        if self.s.get(self.i) == Some(&b'\'') {
                            out.push(b'\'');
                            self.i += 1;
                            continue;
                        }
                        break;
                    }
                    if !(b' '..=b'~').contains(&c) {
                        return Err(format!("non-printable byte in string at {}", self.i - 1));
                    }
                    out.push(c);
                }
                Ok(Value::Str(String::from_utf8_lossy(&out).into_owned()))
            }
            b'.' => {
                self.i += 1;
                let k = self.keyword()?;
                self.eat(b'.')?;
                Ok(Value::Enum(k))
            }
            b'$' => {
                self.i += 1;
                Ok(Value::Unset)
            }
            b'*' => {
                self.i += 1;
                Ok(Value::Derived)
            }
            b'(' => Ok(Value::List(self.list()?)),
            c if c == b'-' || c == b'+' || c.is_ascii_digit() => {
                let st = self.i;
                self.i += 1;
                while self.i < self.s.len() && matches!(self.s[self.i], b'0'..=b'9' | b'.' | b'E' | b'e' | b'-' | b'+')
                {
                    self.i += 1;
                }
                let t = std::str::from_utf8(&self.s[st..self.i]).unwrap();
                let x: f64 = t.replace('E', "e").parse().map_err(|_| format!("bad number {t}"))?;
                Ok(Value::Num(x))
            }
            c if c.is_ascii_alphabetic() => {
                let k = self.keyword()?;
                Ok(Value::Typed(k, self.list()?))
            }
            c => Err(format!("unexpected '{}' at byte {}", c as char, self.i)),
        }
    }
    fn list(&mut self) -> Result<Vec<Value>, String> {
        self.eat(b'(')?;
        let mut v = Vec::new();
        if self.peek() == Some(b')') {
            self.i += 1;
            return Ok(v);
        }
        loop {
            v.push(self.value()?);
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b')') => {
                    self.i += 1;
                    return Ok(v);
                }
                _ => return Err(format!("expected ',' or ')' at byte {}", self.i)),
            }
        }
    }
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

/// Parses a whole exchange file and checks that every reference is defined.
pub fn parse(text: &str) -> Result<Model, String> {
    let s = text.as_bytes();
    if !text.starts_with("ISO-10303-21;") {
        return Err("missing ISO-10303-21;".into());
    }
    if !text.trim_end().ends_with("END-ISO-10303-21;") {
        return Err("missing END-ISO-10303-21;".into());
    }
    let h = find(s, b"HEADER;").ok_or("missing HEADER")?;
    let d = find(s, b"\nDATA;").ok_or("missing DATA")?;
    let mut m = Model::default();
    let header = &text[h..d];
    if let Some(k) = header.find("FILE_SCHEMA((") {
        let rest = &header[k + 14..];
        m.schema = rest[..rest.find('\'').unwrap_or(0)].to_string();
    }
    for need in ["FILE_DESCRIPTION(", "FILE_NAME(", "FILE_SCHEMA("] {
        if !header.contains(need) {
            return Err(format!("header without {need}"));
        }
    }
    let mut lx = Lx { s, i: d + 6 };
    loop {
        match lx.peek() {
            Some(b'#') => {}
            Some(b'E') => {
                let k = lx.keyword()?;
                if k != "ENDSEC" {
                    return Err(format!("unexpected {k}"));
                }
                lx.eat(b';')?;
                break;
            }
            _ => return Err(format!("expected entity at byte {}", lx.i)),
        }
        lx.i += 1;
        let id = lx.uint()?;
        lx.eat(b'=')?;
        let mut parts = Vec::new();
        if lx.peek() == Some(b'(') {
            lx.i += 1;
            while lx.peek() != Some(b')') {
                let k = lx.keyword()?;
                parts.push((k, lx.list()?));
            }
            lx.i += 1;
            let names: Vec<&String> = parts.iter().map(|p| &p.0).collect();
            if names.windows(2).any(|w| w[0] >= w[1]) {
                return Err(format!("#{id}: complex instance not in alphabetical order"));
            }
        } else {
            let k = lx.keyword()?;
            parts.push((k, lx.list()?));
        }
        lx.eat(b';')?;
        if m.entities.insert(id, Entity { id, parts }).is_some() {
            return Err(format!("#{id} defined twice"));
        }
    }
    fn walk(v: &Value, out: &mut Vec<u32>) {
        match v {
            Value::Ref(r) => out.push(*r),
            Value::List(l) | Value::Typed(_, l) => l.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    for e in m.entities.values() {
        let mut rs = Vec::new();
        for p in &e.parts {
            p.1.iter().for_each(|v| walk(v, &mut rs));
        }
        if let Some(r) = rs.iter().find(|r| !m.entities.contains_key(r)) {
            return Err(format!("#{} refers to undefined #{r}", e.id));
        }
    }
    Ok(m)
}
