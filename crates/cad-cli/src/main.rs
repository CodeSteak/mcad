//! `cad`: command line interface for the CAD language (native SMLib kernel), made for agents
//! that develop and check models without the UI.
//!
//! Commands (Plan section 8): `check`, `outline`, `eval`, `inspect --line n`,
//! `render -o bild.png`, `measure`, `watch`, `set`, `export -o teil.stl|teil.3mf|teil.step`,
//! `diff-kernels <files…>` (SMLib ↔ solidcore, see `diff.rs`). Every command prints readable
//! text, or with `--json` one JSON object (`watch`: one object per line). The evaluation runs
//! in a thread with a 256 MiB stack.
//!
//! Daemon (`daemon.rs`, `client.rs`, `host.rs`, `config.rs`): `check`, `eval`, `inspect`,
//! `render`, `export`, `outline`, `measure` and `set` run in a background process that keeps
//! the engine cache (started by the first such call, ends after 15 min without a request);
//! the output is the same as without it. `--no-daemon`, `CAD_NO_DAEMON=1` or
//! `[daemon] enabled = false` switch it off; `cad daemon status|stop|serve`.
//!
//! Desktop UI (feature `gui`, on by default; `launch.rs`): `cad` without arguments starts the
//! UI, `cad DATEI.scad` / `cad ORDNER/` with that file or folder. Without a graphical session
//! (no `DISPLAY`/`WAYLAND_DISPLAY`) or in a build without `gui`: message with the headless
//! commands, exit 2. `cad ui status|show|screenshot` talks to running windows (`ui.rs`, exit 3
//! without one). `cad engine-host` (hidden) is the engine process of the desktop app.
//!
//! Exit codes: 0 ok, 1 diagnostics with severity error (syntax, evaluation or a failed kernel
//! operation; `set`: change refused), 2 usage error (arguments, unreadable file, unknown
//! reference), 3 kernel/internal error (tessellation or export failed, panic).
//!
//! JSON schema (`schema_version` = `cad_session::PROTOCOL_VERSION`, currently 1). Types such
//! as `Diagnostic`, `Scene`, `SrcRange`, `Hint`, `Handle` are the protocol types documented in
//! `web/src/engine/protocol.ts` (lines/columns 1-based, offsets UTF-16); `Bounds` here is
//! `{ min, max, size }`:
//!
//! ```text
//! common:  { schema_version, command, file, ok, exit_code, context: string|null,
//!            diagnostics: Diagnostic[], error?: string,
//!            daemon?: {used: bool, reason: string|null} }   (commands that use the daemon)
//! check:   + { errors: number, warnings: number }
//! outline: + { files: [{path, role}], includes: [{file, line, path, use, found}],
//!            modules: [{name, path, qualified, file, line, end_line,
//!                       params: [{name, default, default_value, type}],
//!                       grids: [Grid'], anchors: [{name, point, dir, line}], errors_with_defaults}],
//!            functions: [{name, params: [{name, default}], file, line, body}],
//!            variables: [{name, expr, value, text, literal, file, line}],
//!            grids: Grid'[] (file level), anchors: [{ref, point, dir, line}],
//!            instances: [{name, module, file, line, depth, part}],
//!            parts: [{name, module, line, node, bounds: Bounds|null, volume}],
//!            counts: {errors, warnings, nodes, modules, parts}, kernel: bool }
//!          Grid' = { name, file, line, planes: [{axis: "x"|"y"|"z", name, value,
//!                    kind: "literal"|"expression"|"reference"|"implicit", expr}] }
//! eval:    + { scene: Scene|null }
//! inspect: + { line, nodes: InspectNode[], hints: Hint[], handles: Handle[], echoes: Echo[] }
//!          InspectNode = { idx, label, instance, range, status, dim, flags, subtracted,
//!                          world_bounds: {min,max}|null, volume|null, area|null,
//!                          selected_edges, opened_faces, hints, messages, cached }
//! render:  + { output, width, height, view, views: [{name, x, y, width, height, camera}],
//!            camera: Camera, line|null, lines: number[], nodes: number[], parts: string[],
//!            highlighted: number[], visible_nodes: [{idx, label, instance, role, line, file,
//!            triangles}], triangles, show: string[], overlays: {grids, grid_points, anchors,
//!            handles, labels, lines, markers}, section: {point, normal, keeps, method}|null,
//!            xray, bounds: Bounds|null, notes: string[],
//!            timings: {evaluate_ms, eval_ms, kernel_ms, render_ms} }
//!          Camera = { projection: "ortho"|"perspective", fov_deg|null, azimuth, elevation,
//!                     dir, up, right, target, eye, scale_px_per_mm, width_mm, height_mm }
//!          role: base | dimmed | highlighted | zone | subtracted | background | modifier #|%
//! measure: + { between?: {a: {ref, kind, point}, b: {…}, delta, distance},
//!              bounds?: {target, nodes, bounds: Bounds|null},
//!              volume?: {target, nodes, volume}, area?: {target, nodes, area},
//!              interference?: {parts, tolerance, method, pairs: [{a, b, bounds_overlap,
//!                              volume, collision?}], collisions: [{a, b, volume}]},
//!              clearance?: {a, b, distance, mesh_distance, point_a, point_b,
//!                           intersection_volume, collision, accuracy, mesh_tol, triangles,
//!                           method} }
//! watch:   one line per run: { schema_version, command, run, trigger: "start"|"change",
//!            changed: string[], file, ok, exit_code, context, errors, warnings,
//!            diagnostics, stats: {nodes, cache_hits, kernel_calls}, volume|null,
//!            bounds: Bounds|null, parts: [{name, volume, bounds}],
//!            render: {output, width, height}|{error}|null,
//!            timings: {total_ms, eval_ms, kernel_ms, render_ms?} }
//! set:     + { changes: [{kind: "param"|"literal", spec, file, line, col, old, new}],
//!            dry_run, written: string[], refused, errors_before, errors_after, diff }
//! export:  + { output, format: "stl"|"3mf"|"step", bytes, triangles, volume|null, area|null,
//!            bounds: {min,max}|null, step?: StepStats }   (triangles 0 for STEP)
//!          with --part/--all-parts: + { parts: [{ name, output, bytes, triangles, volume|null }] }
//! ```
//!
//! Projects: `include <…>`/`use <…>` are read from the file system relative to the including
//! file. Diagnostics of other files are printed with their path (`lib/teile.scad:3:5: …`).

mod client;
mod config;
mod daemon;
mod diff;
mod geom;
mod host;
mod launch;
mod measure;
mod outline;
mod parts;
mod render;
mod set;
mod ui;
mod watch;

use std::ffi::OsString;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cad_session::protocol::{self as p, FromEngine, ToEngine};
use cad_session::{PROTOCOL_VERSION, SessionOptions};
use clap::{Args, Parser, Subcommand};
use glam::{DMat4, DVec3};
use kernel_smlib::SmlibKernel;
use serde_json::{Value as Json, json};

/// Stack of the thread that runs a command (CLI and daemon).
pub(crate) const EVAL_STACK: usize = 256 << 20;

#[derive(Parser)]
#[command(
    name = "cad",
    version,
    about = "Parametrisches CAD: Desktop-UI und headless prüfen, auswerten, ansehen, messen, exportieren",
    override_usage = "cad [DATEI.scad | ORDNER]   (Desktop-UI)\n       cad <BEFEHL> [OPTIONEN]     (headless)",
    allow_external_subcommands = true,
    after_help = "Desktop-UI: `cad` startet die UI, `cad datei.scad` bzw. `cad ordner/` mit dieser Datei bzw. \
diesem Ordner als Projekt (Dateien werden beobachtet: Änderungen von außen erscheinen sofort). Ohne grafische \
Sitzung (kein DISPLAY/WAYLAND_DISPLAY) oder ohne Feature `gui`: Meldung und Exit 2 – dann die Befehle nutzen.\n\
Headless (ohne UI, auch in CI/Containern): alle Befehle unten. Ablauf für Agenten: outline → editieren → check → \
render --views all --show grids,anchors → inspect/measure → export. `cad watch` wertet bei jeder Dateiänderung \
neu aus (Cache bleibt).\n\
Läuft die UI, sieht `cad ui status` was der Nutzer offen hat (Datei, Cursor, Kontextmodul, Parameter, \
Diagnosen), `cad ui show datei --line N` zeigt ihm eine Stelle, `cad ui screenshot -o x.png` liefert seine \
3D-Ansicht.\n\
Daemon: check, eval, inspect, render, export, outline, measure und set rechnen in einem Hintergrundprozess, der den \
Kernel-Cache zwischen Aufrufen hält (startet beim ersten Aufruf, endet nach 15 min ohne Anfrage; Ausgabe wie ohne \
Daemon). Aus: --no-daemon, CAD_NO_DAEMON=1 oder `[daemon] enabled = false` in \
$XDG_CONFIG_HOME/controller-cad/config.toml. Verwalten: `cad daemon status|stop|serve`."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
    /// Ohne Daemon rechnen (dauerhaft: CAD_NO_DAEMON=1 oder `[daemon] enabled = false`).
    #[arg(long, global = true)]
    no_daemon: bool,
    /// Meldungen zum Daemon auf stderr (verwendet oder nicht und warum, Cache-Treffer).
    #[arg(long, global = true)]
    verbose: bool,
}

#[derive(Args, Clone)]
struct Common {
    /// Quelldatei (`-` = stdin).
    file: PathBuf,
    /// Ausgabe als JSON (festes Schema, siehe `cad --help`/Skill).
    #[arg(long)]
    json: bool,
    /// Modul als Kontext (wie Cursor in der Moduldefinition), auch `aussen::innen`.
    #[arg(long, short = 'm')]
    module: Option<String>,
    /// Parameter des Kontextmoduls überschreiben, z. B. `-D w=60`, `-D p=[1,2,3]`.
    #[arg(short = 'D', value_name = "NAME=WERT")]
    define: Vec<String>,
}

#[derive(Subcommand)]
pub(crate) enum Cmd {
    /// Diagnosen mit Zeile und Spalte.
    Check {
        #[command(flatten)]
        c: Common,
        /// Nur Syntax und Sprachauswertung, ohne Kernel.
        #[arg(long)]
        no_kernel: bool,
    },
    /// Übersicht in einem Aufruf: Module mit Parametern, Funktionen, Variablen, Raster und
    /// Anker je Modul, benannte Instanzen, Bauteile, Dateien, Diagnosen.
    Outline {
        #[command(flatten)]
        c: Common,
        /// Ohne Kernel (keine Hüllen/Volumen der Bauteile, schneller).
        #[arg(long)]
        no_kernel: bool,
    },
    /// Baum, Werte, Inline-Hinweise, Raster, Anker.
    Eval {
        #[command(flatten)]
        c: Common,
    },
    /// Was eine Zeile erzeugt: Knoten, Hülle, Volumen, ausgewählte Kanten, Meldungen.
    Inspect {
        #[command(flatten)]
        c: Common,
        /// Zeile (1-basiert).
        #[arg(long)]
        line: u32,
        /// Dateikontext behalten, auch wenn die Zeile in einer Moduldefinition liegt.
        #[arg(long)]
        whole_file: bool,
    },
    /// Bild des Modells (PNG): Ansichten, Kontaktbogen, freie Kamera, Auswahl, Überlagerungen,
    /// Schnitt, Röntgen.
    Render {
        #[command(flatten)]
        c: Common,
        /// Ausgabebild (PNG).
        #[arg(short, long)]
        output: PathBuf,
        #[command(flatten)]
        v: render::ViewArgs,
    },
    /// Messen: Abstand zweier Punkte (Anker, Rasterpunkt, Koordinate), Hülle, Volumen, Fläche,
    /// Kollisionen der Bauteile, Abstand zweier Bauteile.
    Measure {
        #[command(flatten)]
        c: Common,
        #[command(flatten)]
        a: measure::MeasureArgs,
    },
    /// Beobachtet die Datei und eingebundene Dateien; je Auswertung eine JSON-Zeile (mit --json)
    /// bzw. eine Textzeile, optional neues Bild.
    Watch {
        #[command(flatten)]
        c: Common,
        #[command(flatten)]
        a: watch::WatchArgs,
    },
    /// Code ändern über die Engine: Standardwert eines Modulparameters oder ein Literal;
    /// schreibt atomar (`--dry-run`: nur Diff).
    Set {
        #[command(flatten)]
        a: set::SetArgs,
    },
    /// Exportdatei (STL binär, 3MF oder STEP, nach Endung oder --format).
    Export {
        #[command(flatten)]
        c: Common,
        #[arg(short, long)]
        output: PathBuf,
        /// stl, 3mf oder step (Standard: aus der Endung, .stp = step).
        #[arg(long)]
        format: Option<String>,
        /// STEP-Schema: ap242 (Standard) oder ap214.
        #[arg(long, default_value = "ap242")]
        schema: String,
        /// Nur dieses Bauteil (benannte Instanz auf oberster Ebene, z. B. `deckel`, `k[2]`);
        /// mehrfach: alle in eine Datei (3MF: ein Objekt je Bauteil, STEP: ein Produkt).
        #[arg(long = "part", value_name = "NAME")]
        part: Vec<String>,
        /// Jedes Bauteil als eigene Datei in den Ordner `-o` (Format: --format, Standard stl).
        #[arg(long)]
        all_parts: bool,
    },
    /// Vergleich SMLib ↔ solidcore je Knoten (Volumen, Flächen, Kanten, ausgewählte Kanten) und
    /// Abdeckung von solidcore (nur Entwicklung).
    DiffKernels {
        /// Quelldateien.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Ausgabe als JSON.
        #[arg(long)]
        json: bool,
    },
    /// Laufende Desktop-UI abfragen und steuern (status, show, screenshot).
    Ui {
        #[command(subcommand)]
        cmd: ui::UiCmd,
    },
    /// Hintergrundprozess mit Kernel-Cache für die Befehle: status, stop, serve.
    Daemon {
        #[command(subcommand)]
        cmd: daemon::DaemonCmd,
    },
    /// Engine-Prozess der Desktop-App (intern: Rahmen auf stdin/stdout).
    #[command(hide = true)]
    EngineHost,
    #[command(external_subcommand)]
    External(Vec<OsString>),
}

/// Names of the subcommands (messages for `cad DATEI` and unknown commands).
const COMMANDS: &str = "check, outline, eval, inspect, render, measure, watch, set, export, ui, daemon, diff-kernels";

/// Failure that ends a command without a regular result.
pub(crate) enum Fail {
    Usage(String),
    Kernel(String),
}

pub(crate) struct Outcome {
    json: Json,
    text: String,
    /// Exit code 1 (diagnostics with errors).
    errors: bool,
}

/// What a command prints and its exit code (computed here or in the daemon).
pub(crate) struct Output {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub code: u8,
}

impl Output {
    fn emit(self) -> ExitCode {
        let mut o = std::io::stdout().lock();
        let _ = o.write_all(&self.stdout);
        let _ = o.flush();
        let _ = std::io::stderr().lock().write_all(&self.stderr);
        ExitCode::from(self.code)
    }
}

/// Whether the daemon computed a command (`daemon` field of the JSON answer).
pub(crate) struct DaemonInfo {
    pub used: bool,
    pub reason: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let Some(cmd) = cli.cmd else {
        // `cad` alone: the desktop UI.
        return launch::run(None);
    };
    match &cmd {
        Cmd::Ui { cmd } => return ui::command(cmd),
        Cmd::Daemon { cmd } => return daemon::command(cmd),
        Cmd::EngineHost => {
            let code = cad_desktop::engine::host_main(
                SmlibKernel::new,
                std::io::stdin(),
                cad_desktop::engine::protocol_stdout(),
            );
            return ExitCode::from(code as u8);
        }
        Cmd::External(args) => {
            let first = args.first().map(|a| a.to_string_lossy().into_owned()).unwrap_or_default();
            let looks_like_path = first.ends_with(".scad") || first.contains('/') || Path::new(&first).exists();
            if looks_like_path && args.len() == 1 {
                return launch::run(Some(Path::new(&first)));
            }
            if looks_like_path {
                eprintln!(
                    "cad: `cad {first}` öffnet die UI und nimmt keine weiteren Argumente. Befehle: {COMMANDS} (cad --help)"
                );
            } else {
                eprintln!("cad: unbekannter Befehl `{first}`. Befehle: {COMMANDS} (cad --help)");
            }
            return ExitCode::from(2);
        }
        _ => {}
    }
    let json_mode = json_mode(&cmd);
    let name = command_name(&cmd);
    let mut info = None;
    if daemon::eligible(&cmd) {
        let argv: Vec<OsString> = std::env::args_os().skip(1).collect();
        match client::run(&argv, uses_stdin(&cmd), cli.no_daemon, cli.verbose) {
            client::Attempt::Done(out) => return out.emit(),
            client::Attempt::Local(i) => info = Some(i),
        }
    }
    let worker = std::thread::Builder::new().stack_size(EVAL_STACK).name("cad-eval".into()).spawn(move || run(&cmd));
    let result = match worker {
        Ok(h) => h.join().unwrap_or_else(|e| Err(Fail::Kernel(format!("interner Fehler: {}", panic_text(&*e))))),
        Err(e) => Err(Fail::Kernel(format!("Thread: {e}"))),
    };
    finish(name, json_mode, result, info.as_ref()).emit()
}

pub(crate) fn panic_text(e: &(dyn std::any::Any + Send)) -> String {
    e.downcast_ref::<String>()
        .cloned()
        .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "Panik".into())
}

/// `--json` of a command.
pub(crate) fn json_mode(cmd: &Cmd) -> bool {
    match cmd {
        Cmd::Check { c, .. }
        | Cmd::Outline { c, .. }
        | Cmd::Eval { c }
        | Cmd::Inspect { c, .. }
        | Cmd::Render { c, .. }
        | Cmd::Measure { c, .. }
        | Cmd::Watch { c, .. }
        | Cmd::Export { c, .. } => c.json,
        Cmd::Set { a } => a.json,
        Cmd::DiffKernels { json, .. } => *json,
        Cmd::External(_) | Cmd::Ui { .. } | Cmd::Daemon { .. } | Cmd::EngineHost => false,
    }
}

/// The command reads its source from stdin (`-`).
fn uses_stdin(cmd: &Cmd) -> bool {
    match cmd {
        Cmd::Check { c, .. }
        | Cmd::Outline { c, .. }
        | Cmd::Eval { c }
        | Cmd::Inspect { c, .. }
        | Cmd::Render { c, .. }
        | Cmd::Measure { c, .. }
        | Cmd::Export { c, .. } => c.file.as_os_str() == "-",
        _ => false,
    }
}

/// Text or JSON of a result and the exit code (the same in the CLI and in the daemon).
pub(crate) fn finish(
    name: &str,
    json_mode: bool,
    result: Result<Outcome, Fail>,
    daemon: Option<&DaemonInfo>,
) -> Output {
    let daemon_json = daemon.map(|d| json!({"used": d.used, "reason": d.reason}));
    let mut stdout = String::new();
    let mut stderr = String::new();
    let code = match result {
        Ok(o) => {
            let code = if o.errors { 1 } else { 0 };
            if o.json.is_null() {
                // the command printed its own output (watch)
                stdout.push_str(&o.text);
            } else if json_mode {
                let mut j = o.json;
                j["exit_code"] = json!(code);
                j["ok"] = json!(code == 0);
                if let Some(d) = daemon_json {
                    j["daemon"] = d;
                }
                stdout.push_str(&serde_json::to_string_pretty(&j).unwrap());
                stdout.push('\n');
            } else {
                stdout.push_str(&o.text);
            }
            code
        }
        Err(f) => {
            let (code, msg) = match f {
                Fail::Usage(m) => (2u8, m),
                Fail::Kernel(m) => (3u8, m),
            };
            if json_mode {
                let mut j = json!({
                    "schema_version": PROTOCOL_VERSION,
                    "command": name,
                    "ok": false,
                    "exit_code": code,
                    "error": msg,
                });
                if let Some(d) = daemon_json {
                    j["daemon"] = d;
                }
                let j = if name == "watch" { serde_json::to_string(&j) } else { serde_json::to_string_pretty(&j) };
                stdout.push_str(&j.unwrap());
                stdout.push('\n');
            } else {
                stderr.push_str(&format!("cad {name}: {msg}\n"));
            }
            code
        }
    };
    Output { stdout: stdout.into_bytes(), stderr: stderr.into_bytes(), code }
}

pub(crate) fn command_name(c: &Cmd) -> &'static str {
    match c {
        Cmd::Check { .. } => "check",
        Cmd::Outline { .. } => "outline",
        Cmd::Eval { .. } => "eval",
        Cmd::Inspect { .. } => "inspect",
        Cmd::Render { .. } => "render",
        Cmd::Measure { .. } => "measure",
        Cmd::Watch { .. } => "watch",
        Cmd::Set { .. } => "set",
        Cmd::Export { .. } => "export",
        Cmd::DiffKernels { .. } => "diff-kernels",
        Cmd::Ui { .. } => "ui",
        Cmd::Daemon { .. } => "daemon",
        Cmd::EngineHost => "engine-host",
        Cmd::External(_) => "ui",
    }
}

pub(crate) fn run(cmd: &Cmd) -> Result<Outcome, Fail> {
    match cmd {
        Cmd::Check { c, no_kernel } => check(c, !no_kernel),
        Cmd::Eval { c } => eval(c),
        Cmd::Inspect { c, line, whole_file } => inspect(c, *line, *whole_file),
        Cmd::Outline { c, no_kernel } => outline::command(c, !no_kernel),
        Cmd::Render { c, output, v } => render::command(c, output, v),
        Cmd::Measure { c, a } => measure::command(c, a),
        Cmd::Watch { c, a } => watch::command(c, a),
        Cmd::Set { a } => set::command(a),
        Cmd::Export { c, output, format, schema, part, all_parts } => {
            if *all_parts || !part.is_empty() {
                parts::export_parts(c, output, format.as_deref(), schema, part, *all_parts)
            } else {
                export(c, output, format.as_deref(), schema)
            }
        }
        Cmd::DiffKernels { files, .. } => diff::command(files),
        Cmd::External(_) | Cmd::Ui { .. } | Cmd::Daemon { .. } | Cmd::EngineHost => unreachable!("handled in main"),
    }
}

// ------------------------------------------------------------------------------------------
// loading
// ------------------------------------------------------------------------------------------

pub(crate) struct Model {
    pub session: host::Pooled,
    pub file: String,
    /// Directory of the main file (paths of other project files are relative to it).
    pub root: PathBuf,
    pub text: String,
    /// Finished scene (`None` on syntax errors).
    pub scene: Option<p::Scene>,
    pub diagnostics: Vec<p::Diagnostic>,
    /// Context module name, `None` for the file.
    pub context: Option<String>,
    /// Source version of the last evaluation (`watch` counts up).
    pub version: u64,
}

impl Model {
    /// Evaluates a new main file text in the same session (context, overrides and the engine
    /// cache stay); project files are read again. Returns the time in ms.
    pub fn reload(&mut self, text: String) -> f64 {
        let t0 = std::time::Instant::now();
        self.version += 1;
        let mut msgs = self.session.post(ToEngine::SetSource { text: text.clone(), version: self.version });
        msgs.extend(self.session.run_to_idle());
        self.diagnostics = last_diagnostics(&msgs).unwrap_or_default();
        // Syntax errors keep the last scene in the session; the CLI reports none.
        let syntax =
            self.diagnostics.iter().any(|d| d.severity == p::Severity::Error && d.source == p::DiagSource::Syntax);
        self.scene = if syntax { None } else { self.session.done().map(|d| d.scene.clone()) };
        self.text = text;
        t0.elapsed().as_secs_f64() * 1000.0
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == p::Severity::Error)
    }

    /// Fields every JSON answer has.
    pub fn base_json(&self, command: &str) -> Json {
        json!({
            "schema_version": PROTOCOL_VERSION,
            "command": command,
            "file": self.file,
            "ok": !self.has_errors(),
            "exit_code": 0,
            "context": self.context,
            "diagnostics": self.diagnostics,
        })
    }

    pub fn diagnostics_text(&self) -> String {
        let mut s = String::new();
        for d in &self.diagnostics {
            let sev = match d.severity {
                p::Severity::Error => "Fehler",
                p::Severity::Warning => "Warnung",
                p::Severity::Info => "Info",
            };
            let src = match d.source {
                p::DiagSource::Syntax => "Syntax",
                p::DiagSource::Eval => "Auswertung",
                p::DiagSource::Kernel => "Kernel",
            };
            let file = match &d.file {
                Some(f) => self.file_display(f),
                None => self.file.clone(),
            };
            s.push_str(&format!("{file}:{}:{}: {sev} ({src}): {}\n", d.range.line, d.range.col, d.message));
        }
        s
    }

    /// A project path as printed (relative to the main file's directory).
    pub fn file_display(&self, path: &str) -> String {
        if Path::new(path).is_absolute() || self.root.as_os_str().is_empty() {
            path.to_string()
        } else {
            self.root.join(path).display().to_string()
        }
    }

    /// Source line `n` (1-based), trimmed.
    pub fn line_text(&self, n: u32) -> &str {
        self.text.lines().nth(n.saturating_sub(1) as usize).unwrap_or("").trim()
    }

    /// "Z.n  text" of a range, with the file for ranges in other project files.
    pub fn range_line(&self, r: &p::SrcRange) -> String {
        match r.file {
            None | Some(0) => format!("Z.{:<4} {}", r.line, self.line_text(r.line)),
            Some(id) => {
                let cur = self.session.current();
                let f = cur.and_then(|c| c.proj.file(id));
                let path = f.map(|f| f.path.clone()).unwrap_or_default();
                let text = f.and_then(|f| f.text.lines().nth(r.line.saturating_sub(1) as usize)).unwrap_or("").trim();
                format!("{path} Z.{} {text}", r.line)
            }
        }
    }
}

fn parse_define(d: &str) -> Result<(String, p::Value), Fail> {
    let (k, v) = d.split_once('=').ok_or_else(|| Fail::Usage(format!("-D {d}: erwartet NAME=WERT")))?;
    let k = k.trim();
    if k.is_empty() {
        return Err(Fail::Usage(format!("-D {d}: Name fehlt")));
    }
    let v = v.trim();
    let val = serde_json::from_str::<p::Value>(v).unwrap_or_else(|_| p::Value::Str(v.to_string()));
    Ok((k.to_string(), val))
}

fn read_source(path: &Path) -> Result<(String, String), Fail> {
    if path.as_os_str() == "-" {
        let s = host::read_stdin().map_err(|e| Fail::Usage(format!("stdin: {e}")))?;
        return Ok(("<stdin>".into(), s));
    }
    let s = std::fs::read_to_string(path).map_err(|e| Fail::Usage(format!("{}: {e}", path.display())))?;
    Ok((path.display().to_string(), s))
}

fn last_diagnostics(msgs: &[FromEngine]) -> Option<Vec<p::Diagnostic>> {
    msgs.iter().rev().find_map(|m| match m {
        FromEngine::Diagnostics { items, .. } => Some(items.clone()),
        _ => None,
    })
}

fn last_scene(msgs: &[FromEngine]) -> Option<p::Scene> {
    msgs.iter().rev().find_map(|m| match m {
        FromEngine::Scene(s) => Some((**s).clone()),
        _ => None,
    })
}

/// Module path name for `SetContext` (`outer::inner`).
fn module_context_name(m: &p::Module) -> String {
    let mut parts = m.path.clone();
    parts.push(m.name.clone());
    parts.join("::")
}

/// Innermost module definition whose body contains line `line`.
fn module_at_line(scene: &p::Scene, line: u32) -> Option<&p::Module> {
    scene
        .modules
        .iter()
        .filter(|m| m.range.line <= line && line <= m.range.end_line)
        .min_by_key(|m| m.range.to - m.range.from)
}

/// Parses, evaluates and (with `kernel`) runs the kernel job.
pub(crate) fn load(c: &Common, line: Option<u32>, whole_file: bool, kernel: bool) -> Result<Model, Fail> {
    let (file, text) = read_source(&c.file)?;
    let defines = c.define.iter().map(|d| parse_define(d)).collect::<Result<Vec<_>, _>>()?;
    let mut opts = SessionOptions { auto_root_mesh: false, ..Default::default() };
    opts.eval.max_stack_bytes = 192 << 20;
    let mut s = host::new_session(opts);
    // Project files (`include`/`use`) relative to the main file's directory.
    let (root, main_name) = if c.file.as_os_str() == "-" {
        (PathBuf::new(), "stdin.scad".to_string())
    } else {
        let root = c.file.parent().map(Path::to_path_buf).unwrap_or_default();
        (root, c.file.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| "main.scad".into()))
    };
    let fs_root = if root.as_os_str().is_empty() { PathBuf::from(".") } else { root.clone() };
    s.set_resolver(Box::new(cad_session::FsResolver { root: fs_root }), &main_name);
    let mut module = c.module.clone();
    if module.is_none()
        && !whole_file
        && let Some(line) = line
    {
        // Cursor in a module definition: that module is the context (Spec: UI-Modell).
        let first = s.post(ToEngine::SetSource { text: text.clone(), version: 1 });
        module = last_scene(&first).and_then(|sc| module_at_line(&sc, line).map(module_context_name));
        s.post(ToEngine::Cancel);
    }
    if module.is_none() && !defines.is_empty() {
        return Err(Fail::Usage("-D braucht einen Modulkontext (--module oder --line in einem Modul)".into()));
    }
    if module.is_some() {
        s.post(ToEngine::SetContext { module: module.clone(), overrides: defines.into_iter().collect() });
    }
    let mut msgs = s.post(ToEngine::SetSource { text: text.clone(), version: 1 });
    if kernel {
        msgs.extend(s.run_to_idle());
    } else {
        s.post(ToEngine::Cancel);
    }
    let diagnostics = last_diagnostics(&msgs).unwrap_or_default();
    let scene = if kernel { s.done().map(|d| d.scene.clone()) } else { last_scene(&msgs) };
    Ok(Model { session: s, file, root, text, scene, diagnostics, context: module, version: 1 })
}

// ------------------------------------------------------------------------------------------
// formatting helpers
// ------------------------------------------------------------------------------------------

pub(crate) fn num(v: f64) -> String {
    let r = (v * 1e4).round() / 1e4;
    let s = format!("{r:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

pub(crate) fn vec3(v: &p::Vec3) -> String {
    format!("[{}, {}, {}]", num(v.0[0]), num(v.0[1]), num(v.0[2]))
}

pub(crate) fn mat(m: &p::Mat4) -> DMat4 {
    DMat4::from_cols_array(&m.0)
}

/// Axis-aligned world bounds of node-system bounds under `m`.
pub(crate) fn world_bounds(b: &p::Bounds, m: &DMat4) -> p::Bounds {
    let (lo, hi) = (DVec3::from_array(b.min.0), DVec3::from_array(b.max.0));
    let mut wlo = DVec3::splat(f64::INFINITY);
    let mut whi = DVec3::splat(f64::NEG_INFINITY);
    for i in 0..8 {
        let c = DVec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let w = m.transform_point3(c);
        wlo = wlo.min(w);
        whi = whi.max(w);
    }
    p::Bounds { min: p::Vec3(wlo.to_array()), max: p::Vec3(whi.to_array()) }
}

/// "Z.n", with the file's path for ranges in other project files.
pub(crate) fn zl(sc: &p::Scene, r: &p::SrcRange) -> String {
    match r.file.and_then(|f| sc.files.get(f as usize)).filter(|f| f.id != 0) {
        Some(f) => format!("{} Z.{}", f.path, r.line),
        None => format!("Z.{}", r.line),
    }
}

fn status_text(s: p::NodeStatus) -> &'static str {
    match s {
        p::NodeStatus::Ok => "ok",
        p::NodeStatus::Warning => "Warnung",
        p::NodeStatus::Error => "Fehler",
        p::NodeStatus::Skipped => "übersprungen",
        p::NodeStatus::Pending => "offen",
    }
}

fn context_text(m: &Model) -> String {
    let Some(sc) = &m.scene else { return String::new() };
    match &sc.context.module {
        None => "Kontext: Datei\n".into(),
        Some(name) => {
            let params: Vec<String> = sc
                .context
                .params
                .iter()
                .map(|pv| {
                    format!("{} = {}{}", pv.name, value_text(&pv.value), if pv.overridden { " (-D)" } else { "" })
                })
                .collect();
            format!("Kontext: Modul {name}({})\n", params.join(", "))
        }
    }
}

pub(crate) fn value_text(v: &p::Value) -> String {
    match v {
        p::Value::Undef => "undef".into(),
        p::Value::Bool(b) => b.to_string(),
        p::Value::Num(n) => num(*n),
        p::Value::Str(s) => format!("{s:?}"),
        p::Value::List(l) => format!("[{}]", l.iter().map(value_text).collect::<Vec<_>>().join(", ")),
        p::Value::Range { start, step, end } => format!("[{} : {} : {}]", num(*start), num(*step), num(*end)),
        p::Value::Opaque { opaque } => format!("<{opaque}>"),
    }
}

// ------------------------------------------------------------------------------------------
// commands
// ------------------------------------------------------------------------------------------

fn check(c: &Common, kernel: bool) -> Result<Outcome, Fail> {
    let m = load(c, None, true, kernel)?;
    let errors = m.diagnostics.iter().filter(|d| d.severity == p::Severity::Error).count();
    let warnings = m.diagnostics.iter().filter(|d| d.severity == p::Severity::Warning).count();
    let mut j = m.base_json("check");
    j["errors"] = json!(errors);
    j["warnings"] = json!(warnings);
    let mut text = m.diagnostics_text();
    text.push_str(&format!("{}: {errors} Fehler, {warnings} Warnungen\n", m.file));
    Ok(Outcome { json: j, text, errors: m.has_errors() })
}

fn eval(c: &Common) -> Result<Outcome, Fail> {
    let m = load(c, None, true, true)?;
    let mut j = m.base_json("eval");
    j["scene"] = serde_json::to_value(&m.scene).unwrap();
    let mut t = String::new();
    if let Some(sc) = &m.scene {
        t.push_str(&context_text(&m));
        t.push_str("Baum:\n");
        if let Some(root) = sc.root {
            tree_text(sc, root, 1, &mut t);
        }
        if !sc.hints.is_empty() {
            t.push_str("Werte:\n");
            for h in sc.hints.iter().filter(|h| h.in_context) {
                t.push_str(&format!("  {}  → {}\n", m.range_line(&h.range), h.text));
            }
        }
        if !sc.parts.is_empty() {
            t.push_str("Bauteile:\n");
            for pt in &sc.parts {
                let b = pt
                    .bounds
                    .as_ref()
                    .map(|b| format!("  Hülle {}–{}", vec3(&b.min), vec3(&b.max)))
                    .unwrap_or_default();
                let v = pt.volume.map(|v| format!("  Volumen {} mm³", num(v))).unwrap_or_default();
                t.push_str(&format!("  {}  Z.{}{b}{v}\n", pt.name, pt.range.line));
            }
        }
        if sc.files.len() > 1 {
            t.push_str("Dateien:\n");
            for f in &sc.files {
                let role = match f.role {
                    p::FileRole::Main => "Hauptdatei",
                    p::FileRole::Include => "include",
                    p::FileRole::Use => "use",
                };
                t.push_str(&format!("  {} ({role})\n", f.path));
            }
        }
        if !sc.grids.is_empty() {
            t.push_str("Raster:\n");
            for g in &sc.grids {
                let module = &sc.instances.get(g.module_inst as usize).map(|i| i.module.as_str()).unwrap_or("");
                t.push_str(&format!(
                    "  {} ({}, {}{}):\n",
                    g.name,
                    zl(sc, &g.range),
                    if module.is_empty() { "Datei".to_string() } else { format!("Modul {module}") },
                    if g.in_context { "" } else { ", nicht im Kontext" }
                ));
                for axis in 0..3u8 {
                    let planes: Vec<String> = g
                        .planes
                        .iter()
                        .filter(|pl| pl.axis == axis)
                        .map(|pl| {
                            let kind = if pl.implicit {
                                ""
                            } else if pl.literal.is_some() {
                                " (Literal)"
                            } else if pl.is_ref {
                                " (Referenz)"
                            } else {
                                " (Ausdruck)"
                            };
                            format!("{} = {}{kind}", pl.name, num(pl.value))
                        })
                        .collect();
                    t.push_str(&format!("    {}: {}\n", ["x", "y", "z"][axis as usize], planes.join(", ")));
                }
            }
        }
        if !sc.anchors.is_empty() {
            t.push_str("Anker (Kontextkoordinaten):\n");
            for a in &sc.anchors {
                let name = a.text.clone().unwrap_or_else(|| {
                    let module = sc.instances.get(a.inst as usize).map(|i| i.module.clone()).unwrap_or_default();
                    format!("{module}#{}.{}", a.inst, a.name)
                });
                let dir = a.dir.as_ref().map(|d| format!(", dir {}", vec3(d))).unwrap_or_default();
                t.push_str(&format!("  {name} = {}{dir}  ({})\n", vec3(&a.point), zl(sc, &a.range)));
            }
        }
        if !sc.echoes.is_empty() {
            t.push_str("Echo:\n");
            for e in &sc.echoes {
                t.push_str(&format!("  {}: ECHO: {}\n", zl(sc, &e.range), e.text));
            }
        }
        t.push_str(&format!(
            "Statistik: {} Knoten, {} aus dem Cache, {} Kernel-Aufrufe, Auswertung {} ms, Kernel {} ms\n",
            sc.stats.nodes,
            sc.stats.cache_hits,
            sc.stats.kernel_calls,
            num(sc.stats.eval_ms),
            num(sc.stats.kernel_ms)
        ));
    }
    t.push_str(&m.diagnostics_text());
    Ok(Outcome { json: j, text: t, errors: m.has_errors() })
}

fn tree_text(sc: &p::Scene, n: u32, depth: usize, out: &mut String) {
    let node = &sc.nodes[n as usize];
    let mut line = format!("{}[{}] {}", "  ".repeat(depth), node.idx, node.label);
    if let Some(i) = &node.instance {
        line = format!("{line} ({i})");
    }
    if node.stmt.is_some() {
        line.push_str(&format!("  {}", zl(sc, &node.range)));
    }
    let mut flags = Vec::new();
    if node.flags.highlight {
        flags.push("#");
    }
    if node.flags.background {
        flags.push("%");
    }
    if node.flags.disabled {
        flags.push("*");
    }
    if node.flags.zone {
        flags.push("Zone");
    }
    if node.subtracted {
        flags.push("abgezogen");
    }
    if !flags.is_empty() {
        line.push_str(&format!("  [{}]", flags.join(", ")));
    }
    line.push_str(&format!("  {}", status_text(node.status)));
    if let Some(b) = &node.bounds {
        let wb = world_bounds(b, &mat(&node.display_world));
        line.push_str(&format!("  Hülle {}–{}", vec3(&wb.min), vec3(&wb.max)));
    }
    for h in &node.hints {
        line.push_str(&format!("  „{h}“"));
    }
    for msg in &node.messages {
        line.push_str(&format!("  ! {}", msg.message));
    }
    out.push_str(&line);
    out.push('\n');
    for &c in &node.children {
        tree_text(sc, c, depth + 1, out);
    }
}

/// Nodes a line produces: statements starting on the line, otherwise the innermost
/// statements containing it.
pub(crate) fn nodes_at_line(sc: &p::Scene, line: u32) -> Vec<u32> {
    let starts: Vec<u32> =
        sc.nodes.iter().filter(|n| n.stmt.is_some() && n.range.line == line).map(|n| n.idx).collect();
    if !starts.is_empty() {
        return starts;
    }
    let containing: Vec<&p::SceneNode> =
        sc.nodes.iter().filter(|n| n.stmt.is_some() && n.range.line <= line && line <= n.range.end_line).collect();
    let Some(min) = containing.iter().map(|n| n.range.to - n.range.from).min() else { return Vec::new() };
    containing.iter().filter(|n| n.range.to - n.range.from == min).map(|n| n.idx).collect()
}

fn inspect(c: &Common, line: u32, whole_file: bool) -> Result<Outcome, Fail> {
    let mut m = load(c, Some(line), whole_file, true)?;
    let mut j = m.base_json("inspect");
    j["line"] = json!(line);
    let mut t = String::new();
    let Some(sc) = m.scene.clone() else {
        j["nodes"] = json!([]);
        t.push_str(&m.diagnostics_text());
        return Ok(Outcome { json: j, text: t, errors: m.has_errors() });
    };
    t.push_str(&context_text(&m));
    t.push_str(&format!("Z.{line}: {}\n", m.line_text(line)));
    let ids = nodes_at_line(&sc, line);
    let mut nodes = Vec::new();
    for &i in &ids {
        let n = &sc.nodes[i as usize];
        let wb = n.bounds.as_ref().map(|b| world_bounds(b, &mat(&n.display_world)));
        let (volume, area) = if n.dim == Some(p::Dim::Three) && n.status != p::NodeStatus::Skipped {
            match m.session.mass_props(i) {
                Ok(mp) => (Some(mp.volume), Some(mp.area)),
                Err(e) => return Err(Fail::Kernel(e)),
            }
        } else {
            (None, None)
        };
        let mut l = format!("  [{}] {}", n.idx, n.label);
        if let Some(inst) = &n.instance {
            l.push_str(&format!(" ({inst})"));
        }
        l.push_str(&format!(
            "  Z.{}:{}–{}:{}  {}",
            n.range.line,
            n.range.col,
            n.range.end_line,
            n.range.end_col,
            status_text(n.status)
        ));
        if n.flags.zone {
            l.push_str("  [Zone]");
        }
        if n.subtracted {
            l.push_str("  [abgezogen]");
        }
        t.push_str(&l);
        t.push('\n');
        if let Some(b) = &wb {
            t.push_str(&format!("      Hülle {} – {}\n", vec3(&b.min), vec3(&b.max)));
        }
        if let (Some(v), Some(a)) = (volume, area) {
            t.push_str(&format!("      Volumen {} mm³, Fläche {} mm²\n", num(v), num(a)));
        }
        if !n.selected_edges.is_empty() {
            t.push_str(&format!("      ausgewählte Kanten: {}\n", n.selected_edges.len()));
        }
        if n.opened_faces > 0 {
            t.push_str(&format!("      geöffnete Flächen: {}\n", n.opened_faces));
        }
        for h in &n.hints {
            t.push_str(&format!("      Hinweis: {h}\n"));
        }
        for msg in &n.messages {
            let sev = if msg.severity == p::Severity::Error { "Fehler" } else { "Warnung" };
            t.push_str(&format!("      {sev}: {}\n", msg.message));
        }
        nodes.push(json!({
            "idx": n.idx,
            "label": n.label,
            "instance": n.instance,
            "range": n.range,
            "status": n.status,
            "dim": n.dim,
            "flags": n.flags,
            "subtracted": n.subtracted,
            "world_bounds": wb,
            "volume": volume,
            "area": area,
            "selected_edges": n.selected_edges.len(),
            "opened_faces": n.opened_faces,
            "hints": n.hints,
            "messages": n.messages.iter().map(|x| json!({"severity": x.severity, "message": x.message})).collect::<Vec<_>>(),
            "cached": n.cached,
        }));
    }
    if ids.is_empty() {
        t.push_str("  keine Geometrie\n");
    }
    let hints: Vec<&p::Hint> = sc.hints.iter().filter(|h| h.range.line == line).collect();
    for h in &hints {
        t.push_str(&format!("  Wert: {}{}\n", h.text, if h.literal.is_some() { " (Literal, ziehbar)" } else { "" }));
    }
    let handles: Vec<&p::Handle> = sc.handles.iter().filter(|h| h.range.line == line).collect();
    for h in &handles {
        let editable = h.comps.iter().filter(|c| c.is_some()).count();
        let what = if editable > 0 {
            format!("{editable} Literal(e)")
        } else if h.has_ref {
            "Referenz".into()
        } else {
            "gesperrt".into()
        };
        t.push_str(&format!(
            "  Griff {:?} #{}: Wert {:?} bei {} ({what})\n",
            h.kind,
            h.index,
            h.value.iter().map(|v| num(*v)).collect::<Vec<_>>(),
            vec3(&h.world_pos)
        ));
    }
    let echoes: Vec<&p::Echo> = sc.echoes.iter().filter(|e| e.range.line == line).collect();
    for e in &echoes {
        t.push_str(&format!("  ECHO: {}\n", e.text));
    }
    let line_diags: Vec<&p::Diagnostic> =
        m.diagnostics.iter().filter(|d| d.range.line <= line && line <= d.range.end_line).collect();
    for d in &line_diags {
        t.push_str(&format!("  {:?}: {}\n", d.severity, d.message));
    }
    j["nodes"] = json!(nodes);
    j["hints"] = json!(hints);
    j["handles"] = json!(handles);
    j["echoes"] = json!(echoes);
    let errors = m.has_errors();
    m.scene = Some(sc);
    Ok(Outcome { json: j, text: t, errors })
}

fn export(c: &Common, output: &Path, format: Option<&str>, schema: &str) -> Result<Outcome, Fail> {
    let fmt = match format
        .map(|s| s.to_ascii_lowercase())
        .or_else(|| output.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()))
    {
        Some(f) if f == "stl" => p::ExportFormat::Stl,
        Some(f) if f == "3mf" => p::ExportFormat::ThreeMf,
        Some(f) if f == "step" || f == "stp" => p::ExportFormat::Step,
        Some(f) => return Err(Fail::Usage(format!("unbekanntes Exportformat: {f} (stl, 3mf oder step)"))),
        None => return Err(Fail::Usage("Exportformat fehlt (Endung .stl/.3mf/.step oder --format)".into())),
    };
    let schema = cad_engine::StepSchema::parse(schema)
        .ok_or_else(|| Fail::Usage(format!("unbekanntes STEP-Schema: {schema} (ap242 oder ap214)")))?;
    let mut m = load(c, None, true, true)?;
    let mut j = m.base_json("export");
    if m.scene.is_none() {
        let t = m.diagnostics_text();
        return Ok(Outcome { json: j, text: t, errors: true });
    }
    let (bytes, triangles, step) = if fmt == p::ExportFormat::Step {
        let name = output.file_stem().and_then(|s| s.to_str()).unwrap_or("part").to_string();
        let opts = cad_engine::StepOptions { schema, name, timestamp: utc_timestamp(), ..Default::default() };
        let (bytes, st) = m.session.export_step(&opts).map_err(Fail::Kernel)?;
        (bytes, 0, Some(st))
    } else {
        let (bytes, tris) = m.session.export_bytes(fmt).map_err(Fail::Kernel)?;
        (bytes, tris, None)
    };
    host::write_file(output, &bytes).map_err(|e| Fail::Usage(format!("{}: {e}", output.display())))?;
    let root = m.scene.as_ref().and_then(|s| s.root);
    let mp = root.and_then(|r| m.session.mass_props(r).ok());
    let bounds =
        root.and_then(|r| m.scene.as_ref().and_then(|s| s.nodes.get(r as usize)).and_then(|n| n.bounds.clone()));
    j["output"] = json!(output.display().to_string());
    j["format"] = json!(fmt);
    j["bytes"] = json!(bytes.len());
    j["triangles"] = json!(triangles);
    j["volume"] = json!(mp.map(|mp| mp.volume));
    j["area"] = json!(mp.map(|mp| mp.area));
    j["bounds"] = json!(bounds);
    if let Some(st) = &step {
        j["step"] = json!(st);
    }
    let mut t = m.diagnostics_text();
    let what = match &step {
        Some(st) => format!("{} Körper, {} Flächen, {} Kanten", st.solids + st.sheets, st.faces, st.edges),
        None => format!("{triangles} Dreiecke"),
    };
    t.push_str(&format!(
        "{}: {what}, {} Bytes{}\n",
        output.display(),
        bytes.len(),
        mp.map(|mp| format!(", Volumen {} mm³", num(mp.volume))).unwrap_or_default()
    ));
    Ok(Outcome { json: j, text: t, errors: m.has_errors() })
}

/// Current UTC time as ISO 8601 (STEP header).
pub(crate) fn utc_timestamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = (secs / 86400, secs % 86400);
    // civil from days (Howard Hinnant)
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}
