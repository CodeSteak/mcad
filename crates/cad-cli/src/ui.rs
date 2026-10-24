//! `cad ui status|show|screenshot`: talks to running desktop windows over the local IPC
//! (`cad_desktop::uictl`, socket `ui-<pid>` in the user's runtime directory). Without a
//! running UI: message and exit code 3. Headless commands do not need any of this.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use cad_desktop::uictl;
use clap::Subcommand;
use serde_json::{Value as Json, json};

#[derive(Subcommand, Clone)]
pub enum UiCmd {
    /// Running windows: open files, cursor line, selected node, context module, values of the
    /// parameter bar, diagnostics.
    Status {
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Raise the window, open the file (or switch to it), set the cursor.
    Show {
        /// File to show.
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Line (1-based).
        #[arg(long)]
        line: Option<u32>,
        /// Module as context (cursor into its definition).
        #[arg(long, short = 'm')]
        module: Option<String>,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Image of the 3D view as the user sees it (PNG).
    Screenshot {
        /// Output image (PNG).
        #[arg(short, long, value_name = "IMAGE.png")]
        output: PathBuf,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
}

const NO_UI: &str = "keine laufende Desktop-UI (starten mit `cad` oder `cad datei.scad`; headless: cad check/render/…)";

fn fail(json_mode: bool, cmd: &str, code: u8, msg: &str) -> ExitCode {
    if json_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": cad_session::PROTOCOL_VERSION, "command": format!("ui {cmd}"),
                "ok": false, "exit_code": code, "error": msg,
            }))
            .unwrap()
        );
    } else {
        eprintln!("cad ui {cmd}: {msg}");
    }
    ExitCode::from(code)
}

/// Picks the window for a file: the one that has it open, else the newest process.
fn pick(file: Option<&Path>) -> Result<Vec<(String, PathBuf)>, String> {
    let mut all = uictl::running().map_err(|e| e.to_string())?;
    if all.is_empty() {
        return Err(NO_UI.into());
    }
    // newest first (highest pid is not reliable; use the socket's mtime)
    all.sort_by_key(|(_, p)| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
    if let Some(f) = file {
        let f = std::path::absolute(f).unwrap_or_else(|_| f.to_path_buf());
        let has = |path: &Path| {
            uictl::request(path, uictl::STATUS, Json::Null, Duration::from_secs(5)).ok().is_some_and(|r| {
                r.body()["windows"].as_array().is_some_and(|ws| {
                    ws.iter().any(|w| {
                        let root = w["project"]["root"].as_str().map(PathBuf::from);
                        w["project"]["files"].as_array().is_some_and(|fs| {
                            fs.iter()
                                .filter_map(Json::as_str)
                                .any(|rel| root.as_ref().is_some_and(|r| cad_desktop::files::abs_path(r, rel) == f))
                        })
                    })
                })
            })
        };
        if let Some(i) = all.iter().position(|(_, p)| has(p)) {
            let w = all.remove(i);
            all.insert(0, w);
        }
    }
    Ok(all)
}

fn status_text(r: &Json) -> String {
    let mut t = String::new();
    t.push_str(&format!("UI pid {} ({})\n", r["pid"], r["endpoint"].as_str().unwrap_or("")));
    for w in r["windows"].as_array().into_iter().flatten() {
        let pr = &w["project"];
        t.push_str(&format!(
            "  Projekt {} (Hauptdatei {}){}\n",
            pr["root"].as_str().unwrap_or("–"),
            pr["main"].as_str().unwrap_or("–"),
            if pr["folder"] == true { ", Ordner" } else { "" }
        ));
        let files: Vec<String> =
            pr["files"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(String::from)).collect();
        let dirty: Vec<String> =
            w["dirty"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(String::from)).collect();
        t.push_str(&format!(
            "  Dateien: {}\n",
            files
                .iter()
                .map(|f| if dirty.contains(f) { format!("{f}*") } else { f.clone() })
                .collect::<Vec<_>>()
                .join(", ")
        ));
        let c = &w["cursor"];
        t.push_str(&format!(
            "  Cursor: {} Zeile {} Spalte {}\n",
            c["file"].as_str().unwrap_or("–"),
            c["line"],
            c["col"]
        ));
        if let Some(sel) = w["selection"]["nodes"].as_array().filter(|a| !a.is_empty()) {
            let s: Vec<String> =
                sel.iter().map(|n| format!("[{}] {}", n["idx"], n["label"].as_str().unwrap_or(""))).collect();
            t.push_str(&format!("  Auswahl: {}\n", s.join(", ")));
        }
        t.push_str(&format!(
            "  Kontext: {}\n",
            w["context"].as_str().map(|m| format!("Modul {m}")).unwrap_or_else(|| "Datei".into())
        ));
        if let Some(p) = w["params"].as_object().filter(|p| !p.is_empty()) {
            let s: Vec<String> = p.iter().map(|(k, v)| format!("{k} = {v}")).collect();
            t.push_str(&format!("  Parameterleiste: {}\n", s.join(", ")));
        }
        if !w["section"].is_null() {
            t.push_str(&format!("  Schnitt: {}\n", w["section"]["cli"].as_str().unwrap_or("")));
        }
        let diags = w["diagnostics"].as_array().cloned().unwrap_or_default();
        let errors = diags.iter().filter(|d| d["severity"] == "error").count();
        let warnings = diags.iter().filter(|d| d["severity"] == "warning").count();
        t.push_str(&format!("  Diagnosen: {errors} Fehler, {warnings} Warnungen\n"));
        for d in diags.iter().take(20) {
            t.push_str(&format!(
                "    {}:{}:{}: {} {}\n",
                d["file"].as_str().unwrap_or(pr["main"].as_str().unwrap_or("")),
                d["range"]["line"],
                d["range"]["col"],
                d["severity"].as_str().unwrap_or(""),
                d["message"].as_str().unwrap_or("")
            ));
        }
        if let Some(sc) = w["scene"].as_object() {
            t.push_str(&format!(
                "  Szene v{} ({}), {} Knoten\n",
                sc.get("version").cloned().unwrap_or(Json::Null),
                if sc.get("complete") == Some(&Json::Bool(true)) { "vollständig" } else { "vorläufig" },
                sc.get("nodes").cloned().unwrap_or(Json::Null)
            ));
        }
    }
    t
}

pub fn command(cmd: &UiCmd) -> ExitCode {
    match cmd {
        UiCmd::Status { json } => {
            let uis = match pick(None) {
                Ok(u) => u,
                Err(e) => return fail(*json, "status", 3, &e),
            };
            let mut out = Vec::new();
            for (_, p) in &uis {
                match uictl::request(p, uictl::STATUS, Json::Null, Duration::from_secs(8)) {
                    Ok(r) => out.push(r.header.body),
                    Err(e) => out.push(json!({ "endpoint": p.display().to_string(), "error": e.to_string() })),
                }
            }
            if *json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "schema_version": cad_session::PROTOCOL_VERSION, "command": "ui status", "ok": true, "exit_code": 0,
                        "uis": out,
                    }))
                    .unwrap()
                );
            } else {
                for r in &out {
                    print!("{}", status_text(r));
                }
            }
            ExitCode::SUCCESS
        }
        UiCmd::Show { file, line, module, json } => {
            let abs = std::path::absolute(file).unwrap_or_else(|_| file.clone());
            if !abs.exists() {
                return fail(*json, "show", 2, &format!("{} gibt es nicht", abs.display()));
            }
            let uis = match pick(Some(&abs)) {
                Ok(u) => u,
                Err(e) => return fail(*json, "show", 3, &e),
            };
            let body = json!({ "file": abs, "line": line, "module": module });
            match uictl::request(&uis[0].1, uictl::SHOW, body, Duration::from_secs(20)) {
                Ok(r) => {
                    if *json {
                        let mut j = r.header.body.clone();
                        j["schema_version"] = json!(cad_session::PROTOCOL_VERSION);
                        j["command"] = json!("ui show");
                        j["ok"] = json!(true);
                        j["exit_code"] = json!(0);
                        println!("{}", serde_json::to_string_pretty(&j).unwrap());
                    } else {
                        println!(
                            "{} in der UI ({}) geöffnet{}",
                            abs.display(),
                            uis[0].0,
                            line.map(|l| format!(", Zeile {l}")).unwrap_or_default()
                        );
                    }
                    ExitCode::SUCCESS
                }
                Err(cad_ipc::Error::Remote(m)) => fail(*json, "show", 1, &m),
                Err(e) => fail(*json, "show", 3, &e.to_string()),
            }
        }
        UiCmd::Screenshot { output, json } => {
            let uis = match pick(None) {
                Ok(u) => u,
                Err(e) => return fail(*json, "screenshot", 3, &e),
            };
            match uictl::request(&uis[0].1, uictl::SCREENSHOT, Json::Null, Duration::from_secs(15)) {
                Ok(r) => {
                    if let Err(e) = std::fs::write(output, &r.data) {
                        return fail(*json, "screenshot", 2, &format!("{}: {e}", output.display()));
                    }
                    let (w, h) = (r.body()["width"].clone(), r.body()["height"].clone());
                    if *json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&json!({
                                "schema_version": cad_session::PROTOCOL_VERSION, "command": "ui screenshot", "ok": true,
                                "exit_code": 0, "output": output.display().to_string(), "width": w, "height": h, "bytes": r.data.len(),
                            }))
                            .unwrap()
                        );
                    } else {
                        println!("{}: {w}×{h} px, {} Bytes", output.display(), r.data.len());
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => fail(*json, "screenshot", 3, &e.to_string()),
            }
        }
    }
}
