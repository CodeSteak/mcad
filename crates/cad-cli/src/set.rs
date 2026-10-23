//! `cad set`: code changes through the engine's semantics, for agents.
//!
//! * `--param modul.param=wert`: writes the default of a module parameter (the edit of "Als
//!   Standard übernehmen"; the module is looked up in the main file, then in used files).
//! * `--literal [datei:]zeile:spalte=wert`: replaces the editable literal at that position
//!   (the literals the UI edits: values of variables, handle components, grid planes,
//!   parameter defaults), in the main file or a project file.
//!
//! The changed project is evaluated again (language only) before anything is written; if it
//! has more errors than before, nothing is written (exit 1) unless `--force`. Files are
//! written atomically (temporary file + rename); `--dry-run` only prints the diff.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cad_session::protocol::{self as p, ToEngine};
use cad_session::{PROTOCOL_VERSION, Session, SessionOptions, std_clock};
use clap::Args;
use kernel_smlib::SmlibKernel;
use serde_json::{Value as Json, json};

use crate::{Common, Fail, Outcome, last_diagnostics, last_scene, load};

#[derive(Args, Clone, Debug)]
pub(crate) struct SetArgs {
    /// Hauptdatei des Projekts.
    pub file: PathBuf,
    /// Ausgabe als JSON.
    #[arg(long)]
    pub json: bool,
    /// Standardwert eines Modulparameters: `modul.param=wert` (auch `aussen::innen.param=…`),
    /// mehrfach möglich.
    #[arg(long, value_name = "MODUL.PARAM=WERT")]
    pub param: Vec<String>,
    /// Literal an einer Stelle ersetzen: `zeile:spalte=wert` (Hauptdatei) oder
    /// `datei:zeile:spalte=wert`; Spalte wie bei `cad check`, irgendwo im Literal.
    #[arg(long, value_name = "[DATEI:]ZEILE:SPALTE=WERT", allow_hyphen_values = true)]
    pub literal: Vec<String>,
    /// Nur den Diff zeigen, nichts schreiben.
    #[arg(long)]
    pub dry_run: bool,
    /// Auch schreiben, wenn die Änderung neue Fehler erzeugt.
    #[arg(long)]
    pub force: bool,
}

struct Change {
    kind: &'static str,
    spec: String,
    file: u32,
    /// Byte range in that file.
    from: usize,
    to: usize,
    text: String,
}

fn errors_of(d: &[p::Diagnostic]) -> usize {
    d.iter().filter(|d| d.severity == p::Severity::Error).count()
}

/// Byte offset of 1-based (line, UTF-16 column) in `text`.
fn offset_of(text: &str, line: u32, col: u32) -> Option<usize> {
    let start: usize = text.split_inclusive('\n').take(line.saturating_sub(1) as usize).map(str::len).sum();
    let l = text.get(start..)?.split('\n').next()?;
    let mut u = 1u32;
    for (i, ch) in l.char_indices() {
        if u >= col {
            return Some(start + i);
        }
        u += ch.len_utf16() as u32;
    }
    (u >= col).then_some(start + l.len())
}

fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    crate::host::write_atomic(path, text.as_bytes())
}

pub(crate) fn command(a: &SetArgs) -> Result<Outcome, Fail> {
    if a.param.is_empty() && a.literal.is_empty() {
        return Err(Fail::Usage("nichts zu ändern: --param modul.param=wert oder --literal zeile:spalte=wert".into()));
    }
    if a.file.as_os_str() == "-" {
        return Err(Fail::Usage("set braucht eine Datei (nicht stdin)".into()));
    }
    let c = Common { file: a.file.clone(), json: a.json, module: None, define: Vec::new() };
    let mut m = load(&c, None, true, false)?;
    let errors_before = errors_of(&m.diagnostics);
    let Some(file_scene) = m.scene.clone() else {
        return Err(Fail::Usage(format!("{}: Syntaxfehler, erst beheben (cad check)", m.file)));
    };
    let cur = m.session.current().ok_or_else(|| Fail::Kernel("keine Auswertung".into()))?;
    let proj = cur.proj.clone();
    let mut changes: Vec<Change> = Vec::new();

    for spec in &a.param {
        let bad = || Fail::Usage(format!("--param {spec}: erwartet modul.param=wert"));
        let (lhs, value) = spec.split_once('=').ok_or_else(bad)?;
        let (module, param) = lhs.trim().rsplit_once('.').ok_or_else(bad)?;
        let value = value.trim();
        if module.is_empty() || param.is_empty() || value.is_empty() {
            return Err(bad());
        }
        let e = cad_eval::set_default_edit_project(&proj, module, param, value).ok_or_else(|| {
            Fail::Usage(format!("--param {spec}: Parameter `{param}` von Modul `{module}` nicht gefunden"))
        })?;
        let l = e.span.local();
        changes.push(Change {
            kind: "param",
            spec: spec.clone(),
            file: e.span.file(),
            from: l.start as usize,
            to: l.end as usize,
            text: e.text,
        });
    }

    for spec in &a.literal {
        let bad = || Fail::Usage(format!("--literal {spec}: erwartet [datei:]zeile:spalte=wert"));
        let (pos, value) = spec.split_once('=').ok_or_else(bad)?;
        let mut it = pos.rsplitn(3, ':');
        let col: u32 = it.next().and_then(|s| s.trim().parse().ok()).ok_or_else(bad)?;
        let line: u32 = it.next().and_then(|s| s.trim().parse().ok()).ok_or_else(bad)?;
        let file_id = match it.next() {
            None => 0,
            Some(f) => {
                let want = std::path::absolute(f).unwrap_or_else(|_| PathBuf::from(f));
                proj.files
                    .iter()
                    .find(|sf| {
                        let p = m.root.join(&sf.path);
                        sf.path == f
                            || std::path::absolute(&p).is_ok_and(|p| p == want)
                            || (sf.id == 0 && want == std::path::absolute(&a.file).unwrap_or_default())
                    })
                    .map(|sf| sf.id)
                    .ok_or_else(|| {
                        let known: Vec<&str> = proj.files.iter().map(|f| f.path.as_str()).collect();
                        Fail::Usage(format!(
                            "--literal {spec}: Datei `{f}` gehört nicht zum Projekt ({})",
                            known.join(", ")
                        ))
                    })?
            }
        };
        let sf = &proj.files[file_id as usize];
        let at = offset_of(&sf.text, line, col)
            .ok_or_else(|| Fail::Usage(format!("--literal {spec}: Position außerhalb der Datei")))?;
        // the context in which the UI edits this line: the module whose definition contains it
        let module = file_scene
            .modules
            .iter()
            .filter(|md| md.range.file.unwrap_or(0) == file_id && md.range.line <= line && line <= md.range.end_line)
            .min_by_key(|md| md.range.to - md.range.from)
            .map(|md| {
                let mut path = md.path.clone();
                path.push(md.name.clone());
                path.join("::")
            });
        let sc = match &module {
            None => file_scene.clone(),
            Some(name) => {
                let msgs =
                    m.session.post(ToEngine::SetContext { module: Some(name.clone()), overrides: BTreeMap::new() });
                m.session.post(ToEngine::Cancel);
                last_scene(&msgs).unwrap_or_else(|| file_scene.clone())
            }
        };
        let mut cands: Vec<p::SrcRange> = Vec::new();
        cands.extend(sc.hints.iter().filter_map(|h| h.literal));
        cands.extend(sc.handles.iter().flat_map(|h| h.comps.iter().flatten().map(|c| c.range)));
        cands.extend(sc.grids.iter().flat_map(|g| g.planes.iter().filter_map(|pl| pl.literal)));
        cands.extend(sc.modules.iter().flat_map(|md| md.params.iter().filter_map(|pa| pa.default_range)));
        let to_bytes =
            |r: &p::SrcRange| (sf.lines.utf16_to_byte(r.from) as usize, sf.lines.utf16_to_byte(r.to) as usize);
        let mut hits: Vec<(usize, usize)> = cands
            .iter()
            .filter(|r| r.file.unwrap_or(0) == file_id)
            .map(to_bytes)
            .filter(|(f, t)| *f <= at && at <= *t && t > f)
            .collect();
        hits.sort_by_key(|(f, t)| t - f);
        let Some(&(from, to)) = hits.first() else {
            let mut on_line: Vec<String> = cands
                .iter()
                .filter(|r| r.file.unwrap_or(0) == file_id && r.line == line)
                .map(|r| {
                    let (f, t) = to_bytes(r);
                    format!("{}:{} `{}`", r.line, r.col, &sf.text[f..t])
                })
                .collect();
            on_line.dedup();
            return Err(Fail::Usage(format!(
                "--literal {spec}: an {line}:{col} steht kein editierbares Literal{}",
                if on_line.is_empty() {
                    String::new()
                } else {
                    format!(" (Literale der Zeile: {})", on_line.join(", "))
                }
            )));
        };
        changes.push(Change {
            kind: "literal",
            spec: spec.clone(),
            file: file_id,
            from,
            to,
            text: value.trim().to_string(),
        });
    }

    // apply per file (back to front), refuse overlapping edits
    let mut texts: BTreeMap<u32, String> = BTreeMap::new();
    let mut by_file: BTreeMap<u32, Vec<&Change>> = BTreeMap::new();
    for ch in &changes {
        by_file.entry(ch.file).or_default().push(ch);
    }
    for (id, chs) in &mut by_file {
        chs.sort_by_key(|c| std::cmp::Reverse(c.from));
        for w in chs.windows(2) {
            if w[1].to > w[0].from {
                return Err(Fail::Usage(format!("Änderungen überlappen: {} und {}", w[1].spec, w[0].spec)));
            }
        }
        let mut s = proj.files[*id as usize].text.clone();
        for ch in chs.iter() {
            s.replace_range(ch.from..ch.to, &ch.text);
        }
        texts.insert(*id, s);
    }

    // evaluate the changed project (language only, files from memory)
    let new_main = texts.get(&0).cloned().unwrap_or_else(|| proj.files[0].text.clone());
    let mut files: BTreeMap<String, String> = BTreeMap::new();
    for f in proj.files.iter().skip(1) {
        files.insert(f.path.clone(), texts.get(&f.id).cloned().unwrap_or_else(|| f.text.clone()));
    }
    let mut s =
        Session::new(SmlibKernel::new(), SessionOptions { auto_root_mesh: false, ..Default::default() }, std_clock());
    s.post(ToEngine::SetFiles { files, main: Some(proj.files[0].path.clone()) });
    let msgs = s.post(ToEngine::SetSource { text: new_main, version: 1 });
    s.post(ToEngine::Cancel);
    let diags_after = last_diagnostics(&msgs).unwrap_or_default();
    let errors_after = errors_of(&diags_after);
    let refused = errors_after > errors_before && !a.force;

    let mut diff = String::new();
    let mut rows = Vec::new();
    for ch in &changes {
        let sf = &proj.files[ch.file as usize];
        let (line, col) = sf.lines.line_col(ch.from as u32);
        rows.push(json!({
            "kind": ch.kind, "spec": ch.spec, "file": m.file_display(&sf.path), "line": line + 1, "col": col + 1,
            "old": &sf.text[ch.from..ch.to], "new": ch.text,
        }));
    }
    let mut written = Vec::new();
    for (id, new) in &texts {
        let sf = &proj.files[*id as usize];
        let path = if *id == 0 { a.file.clone() } else { m.root.join(&sf.path) };
        let shown = path.display().to_string();
        diff.push_str(
            &similar::TextDiff::from_lines(sf.text.as_str(), new.as_str())
                .unified_diff()
                .context_radius(2)
                .header(&shown, &shown)
                .to_string(),
        );
        if !a.dry_run && !refused {
            write_atomic(&path, new).map_err(|e| Fail::Usage(format!("{shown}: {e}")))?;
            written.push(shown);
        }
    }
    let j = json!({
        "schema_version": PROTOCOL_VERSION,
        "command": "set",
        "file": m.file,
        "ok": !refused,
        "exit_code": 0,
        "context": Json::Null,
        "diagnostics": diags_after,
        "changes": rows,
        "dry_run": a.dry_run,
        "written": written,
        "refused": refused,
        "errors_before": errors_before,
        "errors_after": errors_after,
        "diff": diff,
    });
    let mut t = diff.clone();
    if refused {
        t.push_str(&format!(
            "nicht geschrieben: die Änderung erzeugt Fehler ({errors_before} → {errors_after}); --force schreibt trotzdem\n"
        ));
        let mut tmp = m;
        tmp.diagnostics = diags_after;
        t.push_str(&tmp.diagnostics_text());
    } else if a.dry_run {
        t.push_str("(--dry-run: nichts geschrieben)\n");
    } else {
        t.push_str(&format!("geschrieben: {}\n", written.join(", ")));
    }
    Ok(Outcome { json: j, text: t, errors: refused })
}
