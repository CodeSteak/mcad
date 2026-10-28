//! Writing studio settings into the main file's `render { … }` block (UI write-back): small
//! text edits that keep the rest of the file (comments, formatting) as it is.
//!
//! * A setting that exists gets its value replaced (only the value expression).
//! * A new setting goes after the block's last setting of its kind (top-level settings, lights,
//!   materials), on its own line with the block's indentation (inline in a one-line block).
//! * A removed setting/entry goes with its line when it is alone on it.
//! * Without a block, a new one is appended at the end of the file.

use std::collections::BTreeMap;

use cad_i18n::trs;
use cad_syntax::Span;
use cad_syntax::ast::{RenderDecl, RenderItem, RenderSetting, RenderTarget, StmtKind};

use crate::output::TextEdit;
use crate::palette::{self, ColorKind};
use crate::project::{Project, SourceText};
use crate::render_block::{LIGHT_SETTINGS, MATERIAL_SETTINGS, RENDER_SETTINGS};

/// What a material entry applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditTarget {
    /// Colour as written in the block (`red`, `#rrggbb`, `default`).
    Color(String),
    Part(String),
}

/// One change of the render block. Values are source text (a literal); `None` removes.
#[derive(Debug, Clone, PartialEq)]
pub enum RenderEdit {
    Set { key: String, value: Option<String> },
    Light { name: String, key: String, value: Option<String> },
    RemoveLight { name: String },
    Material { target: EditTarget, key: String, value: Option<String> },
    RemoveMaterial { target: EditTarget },
}

const INDENT: &str = "    ";

/// Edits of the main file for `changes` (empty if nothing changes). `Err` for an unknown
/// setting name or a bad light/part name.
pub fn render_edits(proj: &Project, changes: &[RenderEdit]) -> Result<Vec<TextEdit>, String> {
    for c in changes {
        check(c)?;
    }
    let text = proj.main_text();
    let block = proj.main.stmts.iter().find_map(|s| match &s.kind {
        StmtKind::Render(r) if s.span.file() == 0 => Some((s.span, r.as_ref())),
        _ => None,
    });
    let edits = match block {
        None => new_block(text, changes).into_iter().collect(),
        Some((span, r)) => block_edits(text, span, r, changes),
    };
    Ok(edits.into_iter().filter(|e| proj.span_text(e.span).is_some_and(|t| t != e.text)).collect())
}

fn check(c: &RenderEdit) -> Result<(), String> {
    let (key, known): (&str, &[&str]) = match c {
        RenderEdit::Set { key, .. } => (key, &RENDER_SETTINGS),
        RenderEdit::Light { name, key, .. } => {
            ident(name)?;
            (key, &LIGHT_SETTINGS)
        }
        RenderEdit::Material { target, key, .. } => {
            if let EditTarget::Part(p) = target {
                ident(p)?;
            }
            (key, &MATERIAL_SETTINGS)
        }
        RenderEdit::RemoveLight { name } => return ident(name),
        RenderEdit::RemoveMaterial { .. } => return Ok(()),
    };
    if known.contains(&key) {
        Ok(())
    } else {
        Err(cad_i18n::tr!(
            "render: unbekannte Einstellung `{key}` (bekannt: {})",
            "render: unknown setting `{key}` (known: {})",
            known.join(", ")
        ))
    }
}

fn ident(s: &str) -> Result<(), String> {
    let ok = s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok { Ok(()) } else { Err(trs!("render: ungültiger Name", "render: invalid name").to_string()) }
}

/// `"red"` or `lid` as written in a `material` entry.
fn target_text(t: &EditTarget) -> String {
    match t {
        EditTarget::Color(c) => format!("{c:?}"),
        EditTarget::Part(p) => p.clone(),
    }
}

/// Same colour (palette name, hex) or same part.
fn same_target(a: &RenderTarget, b: &EditTarget) -> bool {
    let rgb = |s: &str| {
        palette::parse_color(s).filter(|c| c.kind != ColorKind::Random).map(|c| {
            let x = palette::rgba_bytes(c.rgba);
            [x[0], x[1], x[2]]
        })
    };
    match (a, b) {
        (RenderTarget::Part(p), EditTarget::Part(q)) => p.name == *q,
        (RenderTarget::Color(x, _), EditTarget::Color(y)) => {
            let (x, y) = (x.trim(), y.trim());
            x.eq_ignore_ascii_case(y)
                || (!x.eq_ignore_ascii_case("default")
                    && !y.eq_ignore_ascii_case("default")
                    && rgb(x).is_some()
                    && rgb(x) == rgb(y))
        }
        _ => false,
    }
}

/// The changes grouped: top-level settings, per light, per material (in order of appearance).
/// Values of one level: (key, new value text or `None` = remove).
type Values = Vec<(String, Option<String>)>;

#[derive(Default)]
struct Grouped {
    settings: Values,
    /// (light name, values, the light is removed)
    lights: Vec<(String, Values, bool)>,
    /// (target, values, the entry is removed)
    materials: Vec<(EditTarget, Values, bool)>,
}

fn group(changes: &[RenderEdit]) -> Grouped {
    let mut g = Grouped::default();
    let set = |list: &mut Vec<(String, Option<String>)>, k: &str, v: &Option<String>| {
        list.retain(|(x, _)| x != k);
        list.push((k.to_string(), v.clone()));
    };
    for c in changes {
        match c {
            RenderEdit::Set { key, value } => set(&mut g.settings, key, value),
            RenderEdit::Light { name, key, value } => {
                let i = g.lights.iter().position(|(n, _, _)| n == name).unwrap_or_else(|| {
                    g.lights.push((name.clone(), Vec::new(), false));
                    g.lights.len() - 1
                });
                g.lights[i].2 = false;
                set(&mut g.lights[i].1, key, value);
            }
            RenderEdit::RemoveLight { name } => {
                g.lights.retain(|(n, _, _)| n != name);
                g.lights.push((name.clone(), Vec::new(), true));
            }
            RenderEdit::Material { target, key, value } => {
                let i = g.materials.iter().position(|(t, _, _)| t == target).unwrap_or_else(|| {
                    g.materials.push((target.clone(), Vec::new(), false));
                    g.materials.len() - 1
                });
                g.materials[i].2 = false;
                set(&mut g.materials[i].1, key, value);
            }
            RenderEdit::RemoveMaterial { target } => {
                g.materials.retain(|(t, _, _)| t != target);
                g.materials.push((target.clone(), Vec::new(), true));
            }
        }
    }
    g
}

/// `key = value; key2 = value2;` of the values that are set.
fn settings_text(list: &[(String, Option<String>)]) -> String {
    list.iter().filter_map(|(k, v)| v.as_ref().map(|v| format!("{k} = {v};"))).collect::<Vec<_>>().join(" ")
}

/// A new block at the end of the file (`None`: nothing to write).
fn new_block(text: &str, changes: &[RenderEdit]) -> Option<TextEdit> {
    let g = group(changes);
    let mut lines: Vec<String> = Vec::new();
    for (k, v) in &g.settings {
        if let Some(v) = v {
            lines.push(format!("{k} = {v};"));
        }
    }
    for (name, list, removed) in &g.lights {
        if !removed {
            lines.push(format!("light {name} {{ {} }}", settings_text(list)).replace("{  }", "{ }"));
        }
    }
    for (t, list, removed) in &g.materials {
        if !removed {
            lines.push(format!("material {} {{ {} }}", target_text(t), settings_text(list)).replace("{  }", "{ }"));
        }
    }
    if lines.is_empty() {
        return None;
    }
    let mut out = String::new();
    if !text.is_empty() && !text.ends_with('\n') {
        out.push('\n');
    }
    if !text.trim().is_empty() && !text.ends_with("\n\n") {
        out.push('\n');
    }
    out.push_str("render {\n");
    for l in lines {
        out.push_str(INDENT);
        out.push_str(&l);
        out.push('\n');
    }
    out.push_str("}\n");
    let end = text.len();
    Some(TextEdit { span: Span::new(end, end), text: out })
}

/// Where new items of a block go and how they are written.
struct Place<'t> {
    text: &'t str,
    /// The block's `{` … `}`.
    body: Span,
    /// Indentation of the block's items.
    indent: String,
}

impl Place<'_> {
    fn line_start(&self, pos: usize) -> usize {
        self.text[..pos].rfind('\n').map_or(0, |i| i + 1)
    }

    fn line_end(&self, pos: usize) -> usize {
        self.text[pos..].find('\n').map_or(self.text.len(), |i| pos + i)
    }

    /// Insertion of `item` after the item ending at `after` (`None`: at the start of the body).
    fn insert(&self, after: Option<usize>, item: &str) -> TextEdit {
        let close = self.body.end as usize - 1;
        let at = after.unwrap_or(self.body.start as usize + 1);
        let le = self.line_end(at);
        let rest = &self.text[at..le.min(close.max(at))];
        let alone = le < close && (rest.trim().is_empty() || rest.trim_start().starts_with("//"));
        if alone {
            // on its own line after the anchor's line
            TextEdit { span: Span::new(le, le), text: format!("\n{}{item}", self.indent) }
        } else if after.is_none() && self.text[at..close].trim().is_empty() {
            // empty one-line block `render {}` / `render { }`: before the `}`
            let text = if self.text[..close].ends_with(' ') { format!("{item} ") } else { format!(" {item} ") };
            TextEdit { span: Span::new(close, close), text }
        } else {
            TextEdit { span: Span::new(at, at), text: format!(" {item}") }
        }
    }

    /// Removal of an item (with its line when it is alone on it).
    fn remove(&self, sp: Span) -> TextEdit {
        let (a, b) = (sp.start as usize, sp.end as usize);
        let ls = self.line_start(a);
        let le = self.line_end(b);
        let before = &self.text[ls..a];
        let after = &self.text[b..le];
        if before.trim().is_empty() && (after.trim().is_empty() || after.trim_start().starts_with("//")) {
            let end = if le < self.text.len() { le + 1 } else { le };
            return TextEdit { span: Span::new(ls, end), text: String::new() };
        }
        let ws = after.len() - after.trim_start().len();
        TextEdit { span: Span::new(a, b + ws), text: String::new() }
    }
}

fn local(sp: Span) -> Span {
    sp.local()
}

fn find_setting<'s>(list: &'s [RenderSetting], key: &str) -> Option<&'s RenderSetting> {
    list.iter().rev().find(|s| s.name.name == key)
}

/// Edits of a setting list (`settings` of a light/material or the block's top level).
fn setting_edits(
    place: &Place,
    existing: &[RenderSetting],
    changes: &[(String, Option<String>)],
    insert_after: Option<usize>,
    inline_close: Option<usize>,
    out: &mut Vec<TextEdit>,
) {
    let mut new_items: Vec<String> = Vec::new();
    for (k, v) in changes {
        match (find_setting(existing, k), v) {
            (Some(s), Some(v)) => out.push(TextEdit { span: local(s.value.span), text: v.clone() }),
            (Some(s), None) => out.push(place.remove(local(s.span))),
            (None, Some(v)) => new_items.push(format!("{k} = {v};")),
            (None, None) => {}
        }
    }
    if new_items.is_empty() {
        return;
    }
    match inline_close {
        // inside `light key { … }`: before its `}`
        Some(close) => {
            let ls = place.line_start(close);
            if place.text[ls..close].trim().is_empty() {
                // `}` on its own line: new lines before it, indented like the entry's settings
                let indent = existing
                    .first()
                    .map(|s| {
                        let p = local(s.span).start as usize;
                        let l = place.line_start(p);
                        place.text[l..p].to_string()
                    })
                    .filter(|p| p.trim().is_empty() && !p.is_empty())
                    .unwrap_or_else(|| format!("{}{INDENT}", &place.text[ls..close]));
                let text: String = new_items.iter().map(|i| format!("{indent}{i}\n")).collect();
                out.push(TextEdit { span: Span::new(ls, ls), text });
            } else {
                let before = &place.text[..close];
                let text = if before.ends_with(' ') {
                    format!("{} ", new_items.join(" "))
                } else {
                    format!(" {} ", new_items.join(" "))
                };
                out.push(TextEdit { span: Span::new(close, close), text });
            }
        }
        None => {
            for it in &new_items {
                out.push(place.insert(insert_after, it));
            }
        }
    }
}

fn block_edits(text: &str, _stmt: Span, r: &RenderDecl, changes: &[RenderEdit]) -> Vec<TextEdit> {
    let body = local(r.body);
    let first_item = r.items.first().map(|i| match i {
        RenderItem::Setting(s) => local(s.span).start as usize,
        RenderItem::Light { span, .. } | RenderItem::Material { span, .. } => local(*span).start as usize,
    });
    let indent = match first_item {
        Some(p) => {
            let ls = text[..p].rfind('\n').map_or(0, |i| i + 1);
            let pre = &text[ls..p];
            if pre.trim().is_empty() && !pre.is_empty() { pre.to_string() } else { INDENT.to_string() }
        }
        None => {
            let p = body.start as usize;
            let ls = text[..p].rfind('\n').map_or(0, |i| i + 1);
            let pre: String = text[ls..p].chars().take_while(|c| c.is_whitespace()).collect();
            format!("{pre}{INDENT}")
        }
    };
    let place = Place { text, body, indent };
    let settings: Vec<RenderSetting> = r
        .items
        .iter()
        .filter_map(|i| match i {
            RenderItem::Setting(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    let end_of = |i: &RenderItem| match i {
        RenderItem::Setting(s) => local(s.span).end as usize,
        RenderItem::Light { span, .. } | RenderItem::Material { span, .. } => local(*span).end as usize,
    };
    let last_setting = r.items.iter().filter(|i| matches!(i, RenderItem::Setting(_))).map(end_of).max();
    let last_light = r.items.iter().filter(|i| matches!(i, RenderItem::Light { .. })).map(end_of).max();
    let last_material = r.items.iter().filter(|i| matches!(i, RenderItem::Material { .. })).map(end_of).max();
    let g = group(changes);
    let mut out: Vec<TextEdit> = Vec::new();
    setting_edits(&place, &settings, &g.settings, last_setting, None, &mut out);
    // lights
    let mut new_lights: Vec<String> = Vec::new();
    for (name, list, removed) in &g.lights {
        let found = r.items.iter().rev().find_map(|i| match i {
            RenderItem::Light { name: n, settings, body, span } if n.name == *name => Some((settings, *body, *span)),
            _ => None,
        });
        match (found, removed) {
            (Some((_, _, span)), true) => out.push(place.remove(local(span))),
            (Some((settings, body, _)), false) => {
                let close = local(body).end as usize - 1;
                setting_edits(&place, settings, list, None, Some(close), &mut out);
            }
            (None, false) => {
                new_lights.push(format!("light {name} {{ {} }}", settings_text(list)).replace("{  }", "{ }"))
            }
            (None, true) => {}
        }
    }
    let after = last_light.or(last_setting);
    for l in &new_lights {
        out.push(place.insert(after, l));
    }
    // materials
    let mut new_mats: Vec<String> = Vec::new();
    for (target, list, removed) in &g.materials {
        let found = r.items.iter().rev().find_map(|i| match i {
            RenderItem::Material { target: t, settings, body, span } if same_target(t, target) => {
                Some((settings, *body, *span))
            }
            _ => None,
        });
        match (found, removed) {
            (Some((_, _, span)), true) => out.push(place.remove(local(span))),
            (Some((settings, body, _)), false) => {
                let close = local(body).end as usize - 1;
                setting_edits(&place, settings, list, None, Some(close), &mut out);
            }
            (None, false) => new_mats
                .push(format!("material {} {{ {} }}", target_text(target), settings_text(list)).replace("{  }", "{ }")),
            (None, true) => {}
        }
    }
    let after = last_material.or(last_light).or(last_setting);
    for m in &new_mats {
        out.push(place.insert(after, m));
    }
    merge(out)
}

/// Insertions at the same place become one edit (in the order they were made); of edits that
/// overlap the first is kept.
fn merge(edits: Vec<TextEdit>) -> Vec<TextEdit> {
    let mut by_pos: BTreeMap<(u32, u32), Vec<String>> = BTreeMap::new();
    for e in edits {
        by_pos.entry((e.span.start, e.span.end)).or_default().push(e.text);
    }
    let mut out: Vec<TextEdit> = Vec::new();
    for ((a, b), texts) in by_pos {
        let text: String = if a == b { texts.concat() } else { texts.last().cloned().unwrap_or_default() };
        if out.last().is_some_and(|p: &TextEdit| a < p.span.end) {
            continue;
        }
        out.push(TextEdit { span: Span { start: a, end: b }, text });
    }
    out
}
