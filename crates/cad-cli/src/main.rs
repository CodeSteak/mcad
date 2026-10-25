//! `mcad`: command line interface for the CAD language (native SMLib kernel), made for agents
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
//! the output is the same as without it. `--no-daemon`, `MCAD_NO_DAEMON=1` or
//! `[daemon] enabled = false` switch it off; `mcad daemon status|stop|serve`.
//!
//! Desktop UI (feature `gui`, on by default; `launch.rs`): `mcad` without arguments starts the
//! UI, `mcad DATEI.mcad` (or `.scad`) / `mcad ORDNER/` with that file or folder. Without a graphical session
//! (no `DISPLAY`/`WAYLAND_DISPLAY`) or in a build without `gui`: message with the headless
//! commands, exit 2. `mcad ui status|show|screenshot` talks to running windows (`ui.rs`, exit 3
//! without one). `mcad engine-host` (hidden) is the engine process of the desktop app.
//!
//! `mcad usage [TOPIC]` prints the usage manual for agents (`manuell/*.md`, English, embedded by
//! `build.rs`; `usage.rs`). The `--help` texts are English as well.
//!
//! Exit codes: 0 ok, 1 diagnostics with severity error (syntax, evaluation or a failed kernel
//! operation; `set`: change refused), 2 usage error (arguments, unreadable file, unknown
//! reference), 3 kernel/internal error (tessellation or export failed, panic), 4 `export`
//! stopped by a violated assertion (`assert_keepout`, …; `--ignore-asserts` exports anyway).
//!
//! JSON schema (`schema_version` = `cad_session::PROTOCOL_VERSION`, currently 2). Types such
//! as `Diagnostic`, `Scene`, `SrcRange`, `Hint`, `Handle` are the protocol types documented in
//! `web/src/engine/protocol.ts` (lines/columns 1-based, offsets UTF-16); `Bounds` here is
//! `{ min, max, size }`:
//!
//! ```text
//! common:  { schema_version, command, file, ok, exit_code, context: string|null,
//!            diagnostics: Diagnostic[], error?: string,
//!            daemon?: {used: bool, reason: string|null} }   (commands that use the daemon)
//! check:   + { errors: number, warnings: number, violations: number,
//!            asserts: AssertResult[] (protocol type) }
//! outline: + { files: [{path, role}], includes: [{file, line, path, use, found}],
//!            modules: [{name, path, qualified, file, line, end_line,
//!                       params: [{name, default, default_value, type}],
//!                       grids: [Grid'], anchors: [{name, point, dir, line}], errors_with_defaults}],
//!            functions: [{name, params: [{name, default}], file, line, body}],
//!            variables: [{name, expr, value, text, literal, file, line}],
//!            grids: Grid'[] (file level), anchors: [{ref, point, dir, line}],
//!            instances: [{name, module, file, line, depth, part}],
//!            parts: [{name, module, line, node, bounds: Bounds|null, volume}],
//!            counts: {errors, warnings, nodes, modules, parts}, kernel: bool,
//!            asserts: [{node, kind, line, file, checked, ok, violations: string[], bodies, note}] }
//!          Grid' = { name, file, line, planes: [{axis: "x"|"y"|"z", name, value,
//!                    kind: "literal"|"expression"|"reference"|"implicit", expr}] }
//! eval:    + { scene: Scene|null }
//! inspect: + { line, nodes: InspectNode[], hints: Hint[], handles: Handle[], echoes: Echo[] }
//!          InspectNode = { idx, label, instance, range, status, dim, flags, subtracted,
//!                          world_bounds: {min,max}|null, volume|null, area|null,
//!                          selected_edges: [[x,y,z]…][] (world), selected_edge_count,
//!                          opened_faces, hints, messages, cached,
//!                          assert: AssertResult|null }
//! render:  + { output, width, height, view, views: [{name, x, y, width, height, camera}],
//!            camera: Camera, line|null, lines: number[], nodes: number[], parts: string[],
//!            highlighted: number[], visible_nodes: [{idx, label, instance, role, line, file,
//!            triangles}], triangles, show: string[], overlays: {grids, grid_points, anchors,
//!            handles, labels, lines, markers, asserts, failures}, section: {point, normal, keeps, method}|null,
//!            xray, bounds: Bounds|null, notes: string[],
//!            timings: {evaluate_ms, eval_ms, kernel_ms, render_ms} }
//!          Camera = { projection: "ortho"|"perspective", fov_deg|null, azimuth, elevation,
//!                     dir, up, right, target, eye, scale_px_per_mm, width_mm, height_mm }
//!          role: base | dimmed | highlighted | zone | subtracted | background | modifier #|% |
//!                keepout
//! measure: + { asserts?: (as outline), between?: {a: {ref, kind, point}, b: {…}, delta, distance},
//!              bounds?: {target, nodes, bounds: Bounds|null},
//!              volume?: {target, nodes, volume}, area?: {target, nodes, area},
//!              interference?: {parts, tolerance, method, pairs: [{a, b, bounds_overlap,
//!                              volume, collision?}], collisions: [{a, b, volume}]},
//!              clearance?: {a, b, distance, mesh_distance, point_a, point_b,
//!                           intersection_volume, collision, accuracy, mesh_tol, triangles,
//!                           method},
//!              measurements?: Measurement[]   (--distance/--radius/--angle/--length/
//!                           --area face@…/--body/--wall, in this order; protocol type: kind,
//!                           title, value, unit, label, values: [{key, label, value, unit}],
//!                           lines, guides, points, at, details, picks: [{kind, reference,
//!                           label, point, origin, …}], exact, tol?, section?, text, cli),
//!              section?: {plane: {point, normal}, method, exact, max_dev, area,
//!                         regions: [{area, holes, body, bounds, reference, contours: [{hole,
//!                         area, segments: [{kind, length, start, end, center?, radius?,
//!                         sweep_deg?, radii?, exact, source, dev}]}]}],
//!                         circles: [{center, radius, diameter, full}],
//!                         min_wall: {thickness, from, to, samples_per_segment}|null},
//!              at?: [{point, elements: [{kind, reference, label, point, distance, origin}]}] }
//! watch:   one line per run: { schema_version, command, run, trigger: "start"|"change",
//!            changed: string[], file, ok, exit_code, context, errors, warnings,
//!            diagnostics, stats: {nodes, cache_hits, kernel_calls}, volume|null,
//!            bounds: Bounds|null, parts: [{name, volume, bounds}],
//!            render: {output, width, height}|{error}|null,
//!            timings: {total_ms, eval_ms, kernel_ms, render_ms?} }
//! set:     + { changes: [{kind: "param"|"literal", spec, file, line, col, old, new}],
//!            dry_run, written: string[], refused, errors_before, errors_after, diff }
//! export:  + { output, format: "stl"|"3mf"|"step", bytes, triangles, volume|null, area|null,
//!            bounds: {min,max}|null, step?: StepStats (bodies: labels), notes }   (triangles 0
//!            for STEP; notes: e.g. overlapping bodies)
//!          with --part/--all-parts: + { parts: [{ name, output, bytes, triangles, volume|null }] }
//!          always: + { asserts: AssertResult[], violations, ignored_asserts, aborted }
//!          (aborted: exit 4, nothing written, no output fields)
//! ```
//!
//! Projects: `include <…>`/`use <…>` are read from the file system relative to the including
//! file. Diagnostics of other files are printed with their path (`lib/teile.mcad:3:5: …`).

mod client;
mod colors;
mod config;
mod daemon;
mod diff;
mod host;
mod launch;
mod measure;
mod outline;
mod parts;
mod render;
mod set;
mod ui;
mod usage;
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
    name = "mcad",
    version,
    about = "Parametric CAD: desktop UI, and headless check, evaluate, view, measure, export",
    override_usage = "mcad [FILE.mcad | FILE.scad | FOLDER]    (desktop UI)\n       mcad <COMMAND> [OPTIONS]                (headless)",
    allow_external_subcommands = true,
    after_help = "Model files: `.mcad` (new files), `.scad` opens and evaluates the same way (also in include/use).\n\
Desktop UI: `mcad` starts the UI, `mcad file.mcad` or `mcad folder/` with that file or folder as the \
project (files are watched: outside changes show up at once). Without a graphical session (no \
DISPLAY/WAYLAND_DISPLAY) or without the `gui` feature: message and exit 2 – use the commands instead.\n\
Headless (no UI, also in CI/containers): all commands above. Workflow for agents: outline → edit → check → \
render --views all --show grids,anchors → inspect/measure → export. `mcad watch` re-evaluates on every file \
change (the cache stays). Full manual for agents/LLMs: `mcad usage`.\n\
While the UI runs, `mcad ui status` shows what the user has open (file, cursor, context module, parameters, \
diagnostics), `mcad ui show file --line N` shows them a location, `mcad ui screenshot -o x.png` returns their \
3D view.\n\
Daemon: check, eval, inspect, render, export, outline, measure and set compute in a background process that \
keeps the kernel cache between calls (starts with the first call, ends after 15 min without a request; output \
as without the daemon). Off: --no-daemon, MCAD_NO_DAEMON=1 or `[daemon] enabled = false` in \
$XDG_CONFIG_HOME/mcad/config.toml. Manage: `mcad daemon status|stop|serve`.\n\
Old names before the rename to mcad still work: CAD_* variables (MCAD_* first), \
~/.config/controller-cad/config.toml (if there is no mcad/config.toml)."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
    /// Compute without the daemon (permanently: MCAD_NO_DAEMON=1 or `[daemon] enabled = false`).
    #[arg(long, global = true)]
    no_daemon: bool,
    /// Daemon messages on stderr (used or not and why, cache hits).
    #[arg(long, global = true)]
    verbose: bool,
}

#[derive(Args, Clone)]
struct Common {
    /// Source file (`-` = stdin).
    #[arg(value_name = "FILE")]
    file: PathBuf,
    /// Output as JSON (fixed schema, see `mcad usage json`).
    #[arg(long)]
    json: bool,
    /// Module as context (like the cursor in the module definition), nested `outer.inner`; with
    /// arguments as in a call: `-m 'holder(30, h = 5)'` (expressions see the file's variables).
    #[arg(long, short = 'm')]
    module: Option<String>,
    /// Override parameters of the context module, e.g. `-D w=60`, `-D p=[1,2,3]`; without a
    /// module: top-level assignments of the main file (like `openscad -D`).
    #[arg(short = 'D', value_name = "NAME=VALUE")]
    define: Vec<String>,
}

#[derive(Subcommand)]
pub(crate) enum Cmd {
    /// Diagnostics with line and column.
    Check {
        #[command(flatten)]
        c: Common,
        /// Syntax and language evaluation only, without the kernel.
        #[arg(long)]
        no_kernel: bool,
    },
    /// Overview in one call: modules with parameters, functions, variables, grids and anchors
    /// per module, named instances, parts, files, diagnostics.
    Outline {
        #[command(flatten)]
        c: Common,
        /// Without the kernel (no bounds/volumes of the parts, faster).
        #[arg(long)]
        no_kernel: bool,
    },
    /// Tree, values, inline hints, grids, anchors.
    Eval {
        #[command(flatten)]
        c: Common,
    },
    /// What a line produces: nodes, bounds, volume, selected edges, messages.
    Inspect {
        #[command(flatten)]
        c: Common,
        /// Line (1-based).
        #[arg(long)]
        line: u32,
        /// Keep the file context even if the line is inside a module definition.
        #[arg(long)]
        whole_file: bool,
    },
    /// Image of the model (PNG): views, contact sheet, free camera, selection, overlays,
    /// section, x-ray.
    Render {
        #[command(flatten)]
        c: Common,
        /// Output image (PNG).
        #[arg(short, long, value_name = "IMAGE.png")]
        output: PathBuf,
        #[command(flatten)]
        v: render::ViewArgs,
    },
    /// Measure on the exact B-rep: distance with foot points, radius/diameter, angle, length,
    /// area, body; in a section contours, circles, wall thickness. Also distance of two points
    /// (anchor, grid point, coordinate), bounds, volume, collisions and clearance of parts.
    /// Elements: `vertex@P`, `mid@P`, `center@P`, `edge@P`, `face@P`, `body@P`, `region@P`.
    Measure {
        #[command(flatten)]
        c: Common,
        #[command(flatten)]
        a: measure::MeasureArgs,
    },
    /// Watches the file and included files; one JSON line (with --json) or text line per
    /// evaluation, optionally a new image.
    Watch {
        #[command(flatten)]
        c: Common,
        #[command(flatten)]
        a: watch::WatchArgs,
    },
    /// Change code through the engine: default value of a module parameter or a literal;
    /// writes atomically (`--dry-run`: diff only).
    Set {
        #[command(flatten)]
        a: set::SetArgs,
    },
    /// Export file (binary STL, 3MF or STEP, by extension or --format).
    Export {
        #[command(flatten)]
        c: Common,
        /// Output file (with --all-parts: a folder).
        #[arg(short, long, value_name = "PATH")]
        output: PathBuf,
        /// stl, 3mf or step (default: from the extension, .stp = step).
        #[arg(long)]
        format: Option<String>,
        /// STEP schema: ap242 (default) or ap214.
        #[arg(long, default_value = "ap242")]
        schema: String,
        /// Only this part (named instance at top level, e.g. `lid`, `k[2]`); repeatable: all
        /// into one file (3MF: one object per part, STEP: one product).
        #[arg(long = "part", value_name = "NAME")]
        part: Vec<String>,
        /// Every part as its own file in the folder `-o` (format: --format, default stl).
        #[arg(long)]
        all_parts: bool,
        /// Single-colour 3MF as before (one mesh, no colours and no filament assignment).
        /// Default: multicolour – one object per part (plus one object for material outside the
        /// parts) with one part per colour and filament slots for Bambu Studio/OrcaSlicer.
        #[arg(long)]
        single_color: bool,
        /// Multicolour 3MF with a colour group (`m:colorgroup`) for programs that show 3MF colours.
        /// Bambu Studio then opens a colour dialog: "Cancel" keeps the slots, OK replaces them.
        #[arg(long, conflicts_with_all = ["single_color", "per_color"])]
        color_group: bool,
        /// One STL per colour (and part) in a ZIP file `-o x.zip` (file names
        /// `F<slot>_<colour>.stl`, all in the same coordinate system).
        #[arg(long, conflicts_with_all = ["single_color", "all_parts"])]
        per_color: bool,
        /// STEP: the whole result as one fused body (implicit union of the top level). Default:
        /// one named body per part and per top-level object outside the parts.
        #[arg(long)]
        merge: bool,
        /// Export even if an assertion (`assert_keepout`, `assert_disjoint`, `assert_intersect`,
        /// `assert_touches`) is violated; without it the export stops with exit code 4.
        #[arg(long)]
        ignore_asserts: bool,
    },
    /// Compare SMLib ↔ solidcore per node (volume, faces, edges, selected edges) and coverage of
    /// solidcore (development only).
    DiffKernels {
        /// Source files.
        #[arg(required = true, value_name = "FILES")]
        files: Vec<PathBuf>,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Query and control the running desktop UI (status, show, screenshot).
    Ui {
        #[command(subcommand)]
        cmd: ui::UiCmd,
    },
    /// Background process with the kernel cache for the commands: status, stop, serve.
    Daemon {
        #[command(subcommand)]
        cmd: daemon::DaemonCmd,
    },
    /// Usage manual for agents and LLMs (English Markdown): workflow, commands, language,
    /// JSON. Without a topic: overview and list of topics.
    Usage {
        #[command(flatten)]
        a: usage::UsageArgs,
    },
    /// Engine process of the desktop app (internal: frames on stdin/stdout).
    #[command(hide = true)]
    EngineHost,
    #[command(external_subcommand)]
    External(Vec<OsString>),
}

/// Names of the subcommands (messages for `mcad DATEI` and unknown commands).
const COMMANDS: &str =
    "check, outline, eval, inspect, render, measure, watch, set, export, ui, daemon, usage, diff-kernels";

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
    /// Another exit code (4: `mcad export` stopped by an assertion violation).
    exit: Option<u8>,
}

/// Exit code of `mcad export` when an assertion is violated (nothing written).
pub(crate) const EXIT_ASSERT: u8 = 4;

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
        // `mcad` alone: the desktop UI.
        return launch::run(None);
    };
    match &cmd {
        Cmd::Ui { cmd } => return ui::command(cmd),
        Cmd::Daemon { cmd } => return daemon::command(cmd),
        Cmd::Usage { a } => return usage::command(a),
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
            let looks_like_path = cad_eval::is_model_file(&first) || first.contains('/') || Path::new(&first).exists();
            if looks_like_path && args.len() == 1 {
                return launch::run(Some(Path::new(&first)));
            }
            if looks_like_path {
                eprintln!(
                    "mcad: `mcad {first}` öffnet die UI und nimmt keine weiteren Argumente. Befehle: {COMMANDS} (mcad --help)"
                );
            } else {
                eprintln!("mcad: unbekannter Befehl `{first}`. Befehle: {COMMANDS} (mcad --help)");
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
        Cmd::External(_) | Cmd::Ui { .. } | Cmd::Daemon { .. } | Cmd::Usage { .. } | Cmd::EngineHost => false,
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
            let code = o.exit.unwrap_or(if o.errors { 1 } else { 0 });
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
                stderr.push_str(&format!("mcad {name}: {msg}\n"));
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
        Cmd::Usage { .. } => "usage",
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
        Cmd::Export {
            c,
            output,
            format,
            schema,
            part,
            all_parts,
            single_color,
            per_color,
            color_group,
            merge,
            ignore_asserts,
        } => {
            let pkg = cad_engine::color_export::PackageOptions { color_group: *color_group };
            if *per_color {
                export_per_color(c, output, part, *ignore_asserts)
            } else if *all_parts || !part.is_empty() {
                parts::export_parts(
                    c,
                    output,
                    format.as_deref(),
                    schema,
                    part,
                    *all_parts,
                    (!*single_color).then_some(pkg),
                    *ignore_asserts,
                )
            } else {
                export(c, output, format.as_deref(), schema, (!*single_color).then_some(pkg), *merge, *ignore_asserts)
            }
        }
        Cmd::DiffKernels { files, .. } => diff::command(files),
        Cmd::External(_) | Cmd::Ui { .. } | Cmd::Daemon { .. } | Cmd::Usage { .. } | Cmd::EngineHost => {
            unreachable!("handled in main")
        }
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

/// `-D` values of the command (a later `-D` of the same name wins).
pub(crate) fn defines(c: &Common) -> Result<std::collections::BTreeMap<String, p::Value>, Fail> {
    c.define.iter().map(|d| parse_define(d)).collect()
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

/// Module path name for `SetContext` (`outer.inner`).
fn module_context_name(m: &p::Module) -> String {
    let mut parts = m.path.clone();
    parts.push(m.name.clone());
    parts.join(".")
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
/// `-m NAME` or `-m 'NAME(args)'`: the module name and the argument text (inside the
/// parentheses), if any.
fn split_module_call(m: &str) -> Result<(Option<String>, Option<String>), Fail> {
    let m = m.trim();
    let Some(open) = m.find('(') else { return Ok((Some(m.to_string()), None)) };
    let name = m[..open].trim();
    let Some(args) = m[open + 1..].strip_suffix(')') else {
        return Err(Fail::Usage(format!("-m {m}: erwartet NAME oder NAME(argumente)")));
    };
    if name.is_empty() {
        return Err(Fail::Usage(format!("-m {m}: Modulname fehlt")));
    }
    Ok((Some(name.to_string()), Some(args.to_string())))
}

/// Binds the arguments of `-m 'NAME(args)'` to the parameters of module `name` like a call at
/// the top level of the main file: positional in order, then by name; the expressions see the
/// file's variables and functions. Errors (exit 2): unknown module or parameter, too many
/// positional arguments, a parameter given twice or also with `-D`, an expression that fails.
fn bind_module_args(
    proj: &cad_eval::Project,
    name: &str,
    args: &str,
    defines: &std::collections::BTreeMap<String, p::Value>,
) -> Result<Vec<(String, p::Value)>, Fail> {
    use cad_syntax::ast::StmtKind;
    let call = format!("-m '{name}({args})'");
    let params = cad_eval::module_params(proj, name)
        .ok_or_else(|| Fail::Usage(format!("{call}: Modul `{name}` nicht gefunden")))?;
    let src = format!("m({args});");
    let (f, d) = cad_syntax::parse(&src);
    if let Some(e) = d.iter().find(|d| d.severity == cad_syntax::Severity::Error) {
        return Err(Fail::Usage(format!("{call}: Syntax: {}", e.message)));
    }
    let Some(StmtKind::Call { args: list, .. }) = f.stmts.first().map(|s| &s.kind).filter(|_| f.stmts.len() == 1)
    else {
        return Err(Fail::Usage(format!("{call}: erwartet NAME(argumente)")));
    };
    let mut names: Vec<String> = Vec::new();
    let mut exprs: Vec<String> = Vec::new();
    let mut positional = 0;
    for a in list {
        let pname = match &a.name {
            Some(n) => {
                if !params.contains(&n.name) {
                    let known = if params.is_empty() { "keine".into() } else { params.join(", ") };
                    return Err(Fail::Usage(format!(
                        "{call}: Modul `{name}` hat keinen Parameter `{}` (Parameter: {known})",
                        n.name
                    )));
                }
                n.name.clone()
            }
            None => {
                let Some(p) = params.get(positional) else {
                    return Err(Fail::Usage(format!(
                        "{call}: zu viele Positionsargumente – `{name}` hat {} Parameter ({})",
                        params.len(),
                        params.join(", ")
                    )));
                };
                positional += 1;
                p.clone()
            }
        };
        if names.contains(&pname) {
            return Err(Fail::Usage(format!("{call}: Parameter `{pname}` doppelt angegeben")));
        }
        if defines.contains_key(&pname) {
            return Err(Fail::Usage(format!(
                "{call}: Parameter `{pname}` steht im Aufruf und in -D {pname}=… (nur eines angeben)"
            )));
        }
        names.push(pname);
        exprs.push(src.get(a.value.span.start as usize..a.value.span.end as usize).unwrap_or("").to_string());
    }
    let opts = cad_eval::EvalOptions { max_stack_bytes: 192 << 20, ..Default::default() };
    let mut out = Vec::new();
    for ((pname, v), text) in names.into_iter().zip(cad_eval::eval_top_level(proj, &opts, &exprs)).zip(&exprs) {
        match v {
            Ok(v) => out.push((pname, cad_session::convert::value(&v))),
            Err(e) => return Err(Fail::Usage(format!("{call}: Argument `{pname}` = {text}: {e}"))),
        }
    }
    Ok(out)
}

pub(crate) fn load(c: &Common, line: Option<u32>, whole_file: bool, kernel: bool) -> Result<Model, Fail> {
    let (file, text) = read_source(&c.file)?;
    let defines = defines(c)?;
    let mut opts = SessionOptions { auto_root_mesh: false, ..Default::default() };
    opts.eval.max_stack_bytes = 192 << 20;
    let mut s = host::new_session(opts);
    // Project files (`include`/`use`) relative to the main file's directory.
    let (root, main_name) = if c.file.as_os_str() == "-" {
        (PathBuf::new(), format!("stdin.{}", cad_eval::MODEL_EXTENSION))
    } else {
        let root = c.file.parent().map(Path::to_path_buf).unwrap_or_default();
        (
            root,
            c.file
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| cad_eval::DEFAULT_MAIN.into()),
        )
    };
    let fs_root = if root.as_os_str().is_empty() { PathBuf::from(".") } else { root.clone() };
    s.set_resolver(Box::new(cad_session::FsResolver { root: fs_root }), &main_name);
    let (mut module, call) = match &c.module {
        Some(m) => split_module_call(m)?,
        None => (None, None),
    };
    let mut defines = defines;
    if let (Some(name), Some(args)) = (&module, &call) {
        // `-m 'halter(30, h = 5)'`: the arguments become overrides (language only, no kernel)
        s.post(ToEngine::SetSource { text: text.clone(), version: 1 });
        s.post(ToEngine::Cancel);
        let cur = s.current().ok_or_else(|| Fail::Usage("-m: Datei nicht auswertbar".into()))?;
        let bound = bind_module_args(&cur.proj, name, args, &defines)?;
        defines.extend(bound);
    }
    if module.is_none()
        && !whole_file
        && let Some(line) = line
    {
        // Cursor in a module definition: that module is the context (Spec: UI-Modell).
        let first = s.post(ToEngine::SetSource { text: text.clone(), version: 1 });
        module = last_scene(&first).and_then(|sc| module_at_line(&sc, line).map(module_context_name));
        s.post(ToEngine::Cancel);
    }
    // Without a module context `-D` overrides top-level assignments of the main file.
    if module.is_some() || !defines.is_empty() {
        s.post(ToEngine::SetContext { module: module.clone(), overrides: defines });
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
    let asserts = m.scene.as_ref().map(|sc| sc.asserts.clone()).unwrap_or_default();
    j["violations"] = json!(asserts.iter().map(|a| a.pairs.iter().filter(|p| !p.ok).count()).sum::<usize>());
    j["asserts"] = json!(asserts);
    let mut text = m.diagnostics_text();
    if !asserts.is_empty() {
        let checked = asserts.iter().filter(|a| a.checked).count();
        let bad = asserts.iter().filter(|a| !a.ok).count();
        let tail = if bad > 0 {
            format!(" ({} Verletzung(en); mcad export bricht dann ab, außer mit --ignore-asserts)", j["violations"])
        } else {
            String::new()
        };
        text.push_str(&format!("Asserts: {checked} von {} geprüft, {bad} verletzt{tail}\n", asserts.len()));
    }
    text.push_str(&format!("{}: {errors} Fehler, {warnings} Warnungen\n", m.file));
    Ok(Outcome { json: j, text, errors: m.has_errors(), exit: None })
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
    Ok(Outcome { json: j, text: t, errors: m.has_errors(), exit: None })
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
        return Ok(Outcome { json: j, text: t, errors: m.has_errors(), exit: None });
    };
    t.push_str(&context_text(&m));
    t.push_str(&format!("Z.{line}: {}\n", m.line_text(line)));
    let ids = nodes_at_line(&sc, line);
    let mut nodes = Vec::new();
    for &i in &ids {
        let n = &sc.nodes[i as usize];
        let world = mat(&n.display_world);
        let wb = n.bounds.as_ref().map(|b| world_bounds(b, &world));
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
        if n.flags.keepout {
            l.push_str("  [Sperrvolumen: nicht im Ergebnis]");
        }
        t.push_str(&l);
        t.push('\n');
        if n.label.starts_with("assert_") {
            let (_, at) = asserts_summary(&sc, Some(&[n.idx]));
            t.push_str(&at.replacen("  ", "      ", 1));
            if let Some(a) = sc.asserts.iter().find(|a| a.node == n.idx) {
                for pr in &a.pairs {
                    t.push_str(&format!("        {} {}\n", if pr.ok { "ok:" } else { "VERLETZT:" }, pr.message));
                }
            }
        }
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
            "selected_edges": n.selected_edges.iter().map(|e| e.iter().map(|q| world.transform_point3(DVec3::from_array(q.0)).to_array()).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "selected_edge_count": n.selected_edges.len(),
            "opened_faces": n.opened_faces,
            "hints": n.hints,
            "messages": n.messages.iter().map(|x| json!({"severity": x.severity, "message": x.message})).collect::<Vec<_>>(),
            "cached": n.cached,
            "assert": sc.asserts.iter().find(|a| a.node == n.idx),
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
    Ok(Outcome { json: j, text: t, errors, exit: None })
}

fn export(
    c: &Common,
    output: &Path,
    format: Option<&str>,
    schema: &str,
    multicolor: Option<cad_engine::color_export::PackageOptions>,
    merge: bool,
    ignore_asserts: bool,
) -> Result<Outcome, Fail> {
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
        return Ok(Outcome { json: j, text: t, errors: true, exit: None });
    }
    let assert_note = match assert_gate(&m, &mut j, ignore_asserts) {
        Ok(n) => n,
        Err(o) => return Ok(o),
    };
    let colored = multicolor.is_some() && fmt == p::ExportFormat::ThreeMf;
    let pkg = multicolor.unwrap_or_default();
    let mut notes = Vec::new();
    let (bytes, triangles, step) = if fmt == p::ExportFormat::Step {
        let name = output.file_stem().and_then(|s| s.to_str()).unwrap_or("part").to_string();
        let opts = cad_engine::StepOptions { schema, name, timestamp: utc_timestamp(), ..Default::default() };
        let (bytes, st, sn) = m.session.export_step_with_notes(&opts, merge).map_err(Fail::Kernel)?;
        for g in &sn.fused {
            notes.push(format!(
                "Körper {} überschneiden sich: als ein Körper „{}“ geschrieben (berührende bleiben getrennt; alles verschmolzen: --merge)",
                g.join(", "),
                g.join(" + ")
            ));
        }
        for (name, e) in &sn.apart {
            notes.push(format!(
                "Körper „{name}“ überschneiden sich, Vereinigung gescheitert ({e}): getrennt geschrieben"
            ));
        }
        for (a, b, e) in &sn.unchecked {
            notes.push(format!("Überschneidung von {a} und {b} nicht prüfbar ({e}): getrennt geschrieben"));
        }
        (bytes, 0, Some(st))
    } else if colored {
        let (bytes, st) = m.session.export_colors_with(fmt, None, pkg).map_err(Fail::Kernel)?;
        notes = cad_session::color_export_notes(&st, pkg);
        (bytes, st.triangles, None)
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
        j["notes"] = json!(notes);
    }
    j["multicolor"] = json!(colored);
    if colored {
        j["color_group"] = json!(pkg.color_group);
        j["notes"] = json!(notes);
    }
    let mut colors_text = String::new();
    if colored && let Some(sc) = m.scene.clone() {
        let (colors, regions, text) = colors::table(&mut m, &sc, true)?;
        j["colors"] = colors;
        j["color_regions"] = regions;
        colors_text = text;
    }
    let mut t = m.diagnostics_text();
    t.push_str(&assert_note);
    let what = match &step {
        Some(st) if st.bodies.len() > 1 => format!(
            "{} Körper ({}), {} Flächen, {} Kanten",
            st.solids + st.sheets,
            st.bodies.join(", "),
            st.faces,
            st.edges
        ),
        Some(st) => format!("{} Körper, {} Flächen, {} Kanten", st.solids + st.sheets, st.faces, st.edges),
        None => format!("{triangles} Dreiecke"),
    };
    t.push_str(&format!(
        "{}: {what}, {} Bytes{}\n",
        output.display(),
        bytes.len(),
        mp.map(|mp| format!(", Volumen {} mm³", num(mp.volume))).unwrap_or_default()
    ));
    if colored {
        t.push_str(&format!("3MF mehrfarbig: {} (einfarbig: --single-color)\n", notes.join("; ")));
    } else {
        for n in &notes {
            t.push_str(&format!("Hinweis: {n}\n"));
        }
    }
    t.push_str(&colors_text);
    Ok(Outcome { json: j, text: t, errors: m.has_errors(), exit: None })
}

/// `mcad export --per-color -o x.zip`: one STL per colour (and part) in a ZIP.
fn export_per_color(c: &Common, output: &Path, parts: &[String], ignore_asserts: bool) -> Result<Outcome, Fail> {
    // a ZIP under another extension would be taken for an STL by the slicer
    if !output.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        return Err(Fail::Usage(format!(
            "--per-color schreibt eine ZIP-Datei mit einem STL je Farbe: -o {}",
            output.with_extension("zip").display()
        )));
    }
    let mut m = load(c, None, true, true)?;
    let mut j = m.base_json("export");
    let Some(sc) = m.scene.clone() else {
        let t = m.diagnostics_text();
        return Ok(Outcome { json: j, text: t, errors: true, exit: None });
    };
    let assert_note = match assert_gate(&m, &mut j, ignore_asserts) {
        Ok(n) => n,
        Err(o) => return Ok(o),
    };
    let names = if parts.is_empty() { None } else { Some(parts) };
    let (bytes, st) = m.session.export_colors(p::ExportFormat::Stl, names).map_err(Fail::Kernel)?;
    host::write_file(output, &bytes).map_err(|e| Fail::Usage(format!("{}: {e}", output.display())))?;
    let (colors, regions, colors_text) = colors::table_of(&mut m, &sc, true, names)?;
    j["output"] = json!(output.display().to_string());
    j["format"] = json!("zip");
    j["per_color"] = json!(true);
    j["bytes"] = json!(bytes.len());
    j["triangles"] = json!(st.triangles);
    j["files"] = json!(st.parts);
    j["colors"] = colors;
    j["color_regions"] = regions;
    let mut t = m.diagnostics_text();
    t.push_str(&assert_note);
    t.push_str(&format!(
        "{}: {} STL (eine je Farbe{}), {} Dreiecke, {} Bytes\n",
        output.display(),
        st.parts,
        if st.objects > 1 { " und Bauteil" } else { "" },
        st.triangles,
        bytes.len()
    ));
    t.push_str(&colors_text);
    Ok(Outcome { json: j, text: t, errors: m.has_errors(), exit: None })
}

/// Violated assertions of a loaded model as text lines (`Z.3 assert_disjoint: …`).
pub(crate) fn assert_violations(m: &Model) -> Vec<String> {
    let Some(sc) = &m.scene else { return Vec::new() };
    let mut v = Vec::new();
    for a in &sc.asserts {
        for pr in a.pairs.iter().filter(|p| !p.ok) {
            v.push(format!("{} {}: {}", zl(sc, &a.range), assert_name(a.kind), pr.message));
        }
    }
    v
}

pub(crate) fn assert_name(k: p::AssertKind) -> &'static str {
    match k {
        p::AssertKind::Keepout => "assert_keepout",
        p::AssertKind::Disjoint => "assert_disjoint",
        p::AssertKind::Intersect => "assert_intersect",
        p::AssertKind::Touches => "assert_touches",
    }
}

/// Short form of the assertions of a scene for `outline`, `inspect` and `measure` (also
/// without kernel: then unchecked): JSON rows and text lines `Z.2 assert_keepout(…): …`.
pub(crate) fn asserts_summary(sc: &p::Scene, only: Option<&[u32]>) -> (Json, String) {
    let mut rows = Vec::new();
    let mut t = String::new();
    for n in sc.nodes.iter().filter(|n| n.label.starts_with("assert_")) {
        if only.is_some_and(|o| !o.contains(&n.idx)) {
            continue;
        }
        let r = sc.asserts.iter().find(|a| a.node == n.idx);
        let params = match r {
            Some(a) => match a.kind {
                p::AssertKind::Keepout => format!("clearance = {}", num(a.clearance)),
                p::AssertKind::Touches => format!("tol = {}, contact = \"{}\"", num(a.tol), a.contact),
                _ => String::new(),
            },
            None => String::new(),
        };
        let violations: Vec<&str> =
            r.map(|a| a.pairs.iter().filter(|p| !p.ok).map(|p| p.message.as_str()).collect()).unwrap_or_default();
        let state = match r {
            None => "nicht geprüft (ohne Kernel)".to_string(),
            Some(a) if !a.checked => format!("nicht geprüft ({})", a.note.clone().unwrap_or_default()),
            Some(_) if violations.is_empty() => {
                let infos: Vec<&str> = r
                    .map(|a| a.pairs.iter().filter(|p| p.contact.is_some()).map(|p| p.message.as_str()).collect())
                    .unwrap_or_default();
                if infos.is_empty() { "ok".to_string() } else { format!("ok – {}", infos.join("; ")) }
            }
            Some(_) => format!("verletzt – {}", violations.join("; ")),
        };
        let bodies = r.map(|a| a.bodies).unwrap_or(0);
        t.push_str(&format!(
            "  {} {}({params}): {state}{}\n",
            zl(sc, &n.range),
            n.label,
            if r.is_some_and(|a| a.checked) { format!(" [{bodies} Körper]") } else { String::new() }
        ));
        rows.push(json!({
            "node": n.idx,
            "kind": n.label.trim_start_matches("assert_"),
            "line": n.range.line,
            "file": n.range.file,
            "checked": r.is_some_and(|a| a.checked),
            "ok": r.is_none_or(|a| a.ok),
            "violations": violations,
            "bodies": bodies,
            "note": r.and_then(|a| a.note.clone()),
        }));
    }
    (json!(rows), t)
}

/// `mcad export` and assertions: with violations the export stops (exit code 4) unless
/// `--ignore-asserts`; then the text gets a warning line. JSON: `asserts`, `violations`,
/// `ignored_asserts`, `aborted`.
pub(crate) fn assert_gate(m: &Model, j: &mut Json, ignore: bool) -> Result<String, Outcome> {
    let asserts = m.scene.as_ref().map(|sc| sc.asserts.clone()).unwrap_or_default();
    let v = assert_violations(m);
    j["asserts"] = json!(asserts);
    j["violations"] = json!(v.len());
    j["ignored_asserts"] = json!(ignore && !v.is_empty());
    j["aborted"] = json!(false);
    if v.is_empty() {
        return Ok(String::new());
    }
    if ignore {
        return Ok(format!("Warnung: trotz {} Assert-Verletzung(en) exportiert (--ignore-asserts)\n", v.len()));
    }
    j["aborted"] = json!(true);
    let mut t = m.diagnostics_text();
    t.push_str(&format!("Export abgebrochen: {} Assert-Verletzung(en), nichts geschrieben:\n", v.len()));
    for l in &v {
        t.push_str(&format!("  {l}\n"));
    }
    t.push_str("Trotzdem exportieren: --ignore-asserts\n");
    Err(Outcome { json: j.clone(), text: t, errors: true, exit: Some(EXIT_ASSERT) })
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
