//! Desktop app (Tauri 2). Processes and threads:
//!
//! ```text
//! mcad (app process)                                  mcad engine-host (engine process)
//!   main thread: Tauri event loop, webview (React UI)    engine thread (256 MiB stack):
//!   commands  engine_post ──── frames on stdin ────────▶   Session::post / step(16 ms)
//!   channel   ◀── binary batches (wire.rs) ── stdout ───   SMLib native
//!   cad-watchdog: 20 min without answer → kill + restart engine, BlockKeys (cad_desktop::engine)
//!   cad-watch: file watcher (notify) → events file-changed / file-removed
//!   cad-ui-ipc: socket ui-<pid> (cad_ipc) ← `mcad ui status|show|screenshot`
//! ```
//!
//! The UI is the browser app (`web/`) built with `pnpm build:tauri` and embedded; it detects
//! Tauri and uses `TauriTransport` (engine) and the Tauri platform (files on disk, watcher,
//! git markers, control requests) instead of the worker and IndexedDB.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cad_desktop::engine::{HostStatus, ProcessLauncher, Supervisor, SupervisorOptions};
use cad_desktop::files::{self, OpenedProject, Recent, RecentEntry};
use cad_desktop::git::{GitBases, Hunk};
use cad_desktop::uictl;
use cad_desktop::watcher::{FileWatcher, WatchEvent};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

/// Program name in window titles (as `productName` in `tauri.conf.json`).
const PRODUCT: &str = "mcad";

/// App id before the rename to mcad (configuration directory with the recent files).
const OLD_APP_ID: &str = "dev.controller-cad.desktop";

/// How the app is started (from `mcad`).
#[derive(Debug, Clone)]
pub struct LaunchOptions {
    /// File or folder to open.
    pub open: Option<PathBuf>,
    /// Program and arguments of the engine process (`mcad engine-host`).
    pub engine_program: PathBuf,
    pub engine_args: Vec<OsString>,
    /// Watchdog timeout (default 20 min; `MCAD_WATCHDOG_MS`, old name `CAD_WATCHDOG_MS`, overrides).
    pub watchdog: Duration,
}

struct EngineOut {
    channel: Option<Channel>,
    /// Frames before the webview attached its channel.
    buffered: Vec<Vec<u8>>,
}

struct AppState {
    sup: Arc<Supervisor>,
    out: Arc<Mutex<EngineOut>>,
    watcher: Option<FileWatcher>,
    git: Mutex<GitBases>,
    recent: Recent,
    initial: Mutex<Option<Result<OpenedProject, String>>>,
    replies: Mutex<HashMap<u64, mpsc::Sender<Json>>>,
    next_id: AtomicU64,
    started: Instant,
    endpoint: String,
}

fn send_out(out: &Mutex<EngineOut>, bytes: Vec<u8>) {
    let mut o = out.lock().unwrap();
    match &o.channel {
        Some(c) => {
            if c.send(InvokeResponseBody::Raw(bytes.clone())).is_err() {
                o.buffered.push(bytes);
            }
        }
        None => o.buffered.push(bytes),
    }
}

// ------------------------------------------------------------------------------------------
// commands
// ------------------------------------------------------------------------------------------

/// Attaches the engine channel of the webview (buffered frames are sent first).
#[tauri::command]
fn engine_start(state: State<'_, AppState>, channel: Channel) {
    let mut o = state.out.lock().unwrap();
    for b in o.buffered.drain(..) {
        let _ = channel.send(InvokeResponseBody::Raw(b));
    }
    o.channel = Some(channel);
    drop(o);
    // The webview (re)started: tell it the engine state.
    let s = state.sup.status();
    let f = cad_ipc::Frame::event("host", serde_json::to_value(&s).unwrap_or(Json::Null));
    send_out(&state.out, cad_desktop::wire::frame_bytes(&f));
}

/// UI → engine messages, in order (`ToEngine` JSON; `__debugHang` for the watchdog test).
#[tauri::command]
fn engine_post(state: State<'_, AppState>, msgs: Vec<Json>) {
    state.sup.send_json(msgs);
}

#[tauri::command]
fn engine_status(state: State<'_, AppState>) -> HostStatus {
    state.sup.status()
}

#[derive(Serialize)]
struct AppInfo {
    pid: u32,
    endpoint: String,
    version: &'static str,
    started_ms: f64,
}

#[tauri::command]
fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        pid: std::process::id(),
        endpoint: state.endpoint.clone(),
        version: env!("CARGO_PKG_VERSION"),
        started_ms: state.started.elapsed().as_secs_f64() * 1000.0,
    }
}

/// The project given on the command line (once), or an error text.
#[tauri::command]
fn initial_project(state: State<'_, AppState>) -> Result<Option<OpenedProject>, String> {
    match state.initial.lock().unwrap().take() {
        None => Ok(None),
        Some(r) => r.map(Some),
    }
}

fn opened(state: &AppState, p: &std::path::Path) -> Result<OpenedProject, String> {
    let proj = files::open_path(p).map_err(|e| format!("{}: {e}", p.display()))?;
    state.recent.add(p, proj.folder);
    Ok(proj)
}

#[tauri::command]
fn open_path(state: State<'_, AppState>, path: PathBuf) -> Result<OpenedProject, String> {
    opened(&state, &path)
}

/// File or folder dialog, then open.
#[tauri::command]
async fn open_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
    folder: bool,
) -> Result<Option<OpenedProject>, String> {
    let d = app.dialog().file();
    let picked = if folder {
        d.set_title("Ordner als Projekt öffnen").blocking_pick_folder()
    } else {
        d.set_title("Datei öffnen").add_filter("CAD-Modelle", files::MODEL_EXTENSIONS).blocking_pick_file()
    };
    let Some(p) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    opened(&state, &p).map(Some)
}

#[derive(Deserialize)]
struct FileText {
    path: PathBuf,
    text: String,
}

#[derive(Serialize)]
struct SaveResult {
    saved: Vec<PathBuf>,
    errors: Vec<String>,
}

/// Writes files atomically; the watcher learns the contents first (no reload loop).
#[tauri::command]
fn save_files(state: State<'_, AppState>, files: Vec<FileText>) -> SaveResult {
    let mut res = SaveResult { saved: Vec::new(), errors: Vec::new() };
    for f in files {
        if let Some(w) = &state.watcher {
            w.note_written(&f.path, &f.text);
        }
        match files::write_atomic(&f.path, &f.text) {
            Ok(()) => res.saved.push(f.path),
            Err(e) => res.errors.push(format!("{}: {e}", f.path.display())),
        }
    }
    res
}

/// "Speichern unter": save dialog, write; returns the new path.
#[tauri::command]
async fn save_as(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    dir: Option<PathBuf>,
    text: String,
) -> Result<Option<PathBuf>, String> {
    let mut d = app
        .dialog()
        .file()
        .set_title("Speichern unter")
        .add_filter("CAD-Modelle", files::MODEL_EXTENSIONS)
        .set_file_name(&name);
    if let Some(dir) = dir {
        d = d.set_directory(dir);
    }
    let Some(p) = d.blocking_save_file().and_then(|p| p.into_path().ok()) else { return Ok(None) };
    let p = if p.extension().is_none() { p.with_extension(files::MODEL_EXTENSION) } else { p };
    if let Some(w) = &state.watcher {
        w.note_written(&p, &text);
    }
    files::write_atomic(&p, &text).map_err(|e| format!("{}: {e}", p.display()))?;
    state.recent.add(&p, false);
    Ok(Some(p))
}

#[tauri::command]
fn recent(state: State<'_, AppState>) -> Vec<RecentEntry> {
    state.recent.load().into_iter().filter(|e| e.path.exists()).collect()
}

#[derive(Deserialize)]
struct WatchFile {
    path: PathBuf,
    /// Content as loaded/saved by the UI (`None`: as on disk now).
    text: Option<String>,
}

/// Replaces the watched files (project files of the UI).
#[tauri::command]
fn watch_files(state: State<'_, AppState>, files: Vec<WatchFile>) {
    if let Some(w) = &state.watcher {
        w.set_files(&files.into_iter().map(|f| (f.path, f.text)).collect::<Vec<_>>());
    }
}

/// Line markers of the buffer `text` of `path` against `HEAD` (`null`: no markers).
#[tauri::command]
async fn git_hunks(
    state: State<'_, AppState>,
    path: PathBuf,
    text: String,
    refresh: bool,
) -> Result<Option<Vec<Hunk>>, String> {
    Ok(state.git.lock().unwrap().hunks(&path, &text, refresh))
}

/// Answer of the webview to a control request (`ui-request`).
#[tauri::command]
fn ui_reply(state: State<'_, AppState>, id: u64, result: Json) {
    if let Some(tx) = state.replies.lock().unwrap().remove(&id) {
        let _ = tx.send(result);
    }
}

#[tauri::command]
fn set_title(app: AppHandle, title: String) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title(&title);
    }
}

// ------------------------------------------------------------------------------------------
// control by `mcad ui …`
// ------------------------------------------------------------------------------------------

/// Asks the webview (event `ui-request`) and waits for `ui_reply`.
fn ask_webview(app: &AppHandle, method: &str, body: Json, timeout: Duration) -> Result<Json, String> {
    let state = app.state::<AppState>();
    let id = state.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let (tx, rx) = mpsc::channel();
    state.replies.lock().unwrap().insert(id, tx);
    app.emit_to("main", "ui-request", json!({ "id": id, "method": method, "body": body }))
        .map_err(|e| e.to_string())?;
    let r = rx.recv_timeout(timeout).map_err(|_| "die Oberfläche antwortet nicht".to_string());
    state.replies.lock().unwrap().remove(&id);
    let r = r?;
    match r.get("error").and_then(Json::as_str) {
        Some(e) => Err(e.to_string()),
        None => Ok(r),
    }
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    let s = s.split_once(',').map_or(s, |(_, d)| d);
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let bytes: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c).ok_or("ungültiges Base64")? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

fn serve_control(app: AppHandle, server: cad_ipc::Server) {
    let _ = server.serve(move |f| {
        let state = app.state::<AppState>();
        match f.header.method.as_str() {
            uictl::STATUS => {
                let w = ask_webview(&app, "status", Json::Null, Duration::from_secs(5))?;
                let engine = serde_json::to_value(state.sup.status()).unwrap_or(Json::Null);
                Ok((
                    json!({ "pid": std::process::id(), "endpoint": state.endpoint, "engine": engine, "windows": [w] }),
                    Vec::new(),
                ))
            }
            uictl::SHOW => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.unminimize();
                    let _ = w.show();
                    let _ = w.set_focus();
                }
                let r = ask_webview(&app, "show", f.header.body.clone(), Duration::from_secs(15))?;
                Ok((r, Vec::new()))
            }
            uictl::SCREENSHOT => {
                let r = ask_webview(&app, "screenshot", f.header.body.clone(), Duration::from_secs(10))?;
                let png = b64_decode(r["png"].as_str().ok_or("kein Bild")?)?;
                Ok((json!({ "width": r["width"], "height": r["height"], "mime": "image/png" }), png))
            }
            m => Err(format!("unbekannte Anfrage `{m}`")),
        }
    });
}

// ------------------------------------------------------------------------------------------
// run
// ------------------------------------------------------------------------------------------

/// Linux/Wayland: WebKitGTK's DMA-BUF renderer ends the app with "Error 71 (Protocol error)
/// dispatching to Wayland display" on several compositor/driver combinations (seen with KDE
/// Plasma and the NVIDIA driver). Without it WebKit hands frames to the compositor over shared
/// memory; the 3D view keeps rendering with WebGL. A value set by the user (also `0`) wins.
/// Runs before any thread starts (`set_var`).
#[cfg(target_os = "linux")]
fn wayland_workarounds() {
    const VAR: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    if wayland && std::env::var_os(VAR).is_none() {
        // SAFETY: called first thing in `run`, before the supervisor, GTK or any other thread
        // exists, so nothing reads the environment concurrently.
        unsafe { std::env::set_var(VAR, "1") };
    }
}

/// Starts the app; returns the exit code when the window is closed.
pub fn run(opts: LaunchOptions) -> i32 {
    #[cfg(target_os = "linux")]
    wayland_workarounds();
    let started = Instant::now();
    let out = Arc::new(Mutex::new(EngineOut { channel: None, buffered: Vec::new() }));
    let o2 = out.clone();
    let sink: cad_desktop::engine::Sink = Arc::new(move |f| send_out(&o2, cad_desktop::wire::frame_bytes(&f)));
    // The engine process starts right away (SMLib initialisation overlaps the webview start).
    let watchdog = cad_ipc::env_var("WATCHDOG_MS")
        .and_then(|v| v.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(opts.watchdog);
    let sup = Supervisor::start(
        Arc::new(ProcessLauncher { program: opts.engine_program.clone(), args: opts.engine_args.clone() }),
        sink,
        SupervisorOptions { watchdog, ..Default::default() },
    );
    let initial = opts.open.as_ref().map(|p| files::open_path(p).map_err(|e| format!("{}: {e}", p.display())));
    let endpoint = uictl::endpoint_name(std::process::id());

    let sup_setup = sup.clone();
    let context = tauri::generate_context!();
    // Linux: the app id (`identifier`) as GLib program name, before GTK starts. GTK takes it as
    // the Wayland `app_id` and the X11 `WM_CLASS` (otherwise the binary name `mcad`), so the
    // desktop file `<identifier>.desktop` (packaging/linux) and its icon match the window.
    #[cfg(target_os = "linux")]
    glib::set_prgname(Some(context.config().identifier.as_str()));
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            engine_start,
            engine_post,
            engine_status,
            app_info,
            initial_project,
            open_path,
            open_dialog,
            save_files,
            save_as,
            recent,
            watch_files,
            git_hunks,
            ui_reply,
            set_title,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let h2 = handle.clone();
            let watcher = FileWatcher::new(move |e| {
                let _ = match e {
                    WatchEvent::Changed { path, text } => {
                        h2.emit_to("main", "file-changed", json!({ "path": path, "text": text }))
                    }
                    WatchEvent::Removed { path } => h2.emit_to("main", "file-removed", json!({ "path": path })),
                };
            })
            .map_err(|e| eprintln!("mcad: Dateibeobachter nicht verfügbar: {e}"))
            .ok();
            let config_dir = app.path().app_config_dir().unwrap_or_else(|_| std::env::temp_dir().join("mcad"));
            let recent_file = config_dir.join("recent.json");
            // Before the rename to mcad the app id was `dev.controller-cad.desktop`: take over
            // its list of recent files once.
            if !recent_file.exists()
                && let Some(old) = config_dir.parent().map(|d| d.join(OLD_APP_ID).join("recent.json"))
                && old.exists()
            {
                let _ = std::fs::create_dir_all(&config_dir).and_then(|()| std::fs::copy(&old, &recent_file));
            }
            let recent = Recent::new(recent_file);
            if let Some(Ok(p)) = &initial {
                let src = opts.open.clone().unwrap_or_else(|| p.root.clone());
                recent.add(&src, p.folder);
            }
            app.manage(AppState {
                sup: sup_setup.clone(),
                out: out.clone(),
                watcher,
                git: Mutex::new(GitBases::default()),
                recent,
                initial: Mutex::new(initial.clone()),
                replies: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(0),
                started,
                endpoint: endpoint.clone(),
            });
            let title = match &initial {
                Some(Ok(p)) => format!("{} – {PRODUCT}", p.name),
                _ => PRODUCT.into(),
            };
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title(title)
                .inner_size(1440.0, 900.0)
                .min_inner_size(800.0, 500.0)
                .build()?;
            match cad_ipc::Server::bind(&endpoint) {
                Ok(server) => {
                    std::thread::Builder::new()
                        .name("cad-ui-ipc".into())
                        .spawn(move || serve_control(handle, server))?;
                }
                Err(e) => eprintln!("mcad: Steuerung über `mcad ui` nicht verfügbar: {e}"),
            }
            Ok(())
        })
        .build(context);
    let app = match app {
        Ok(a) => a,
        Err(e) => {
            eprintln!("mcad: Desktop-App startet nicht: {e}");
            sup.shutdown();
            return 3;
        }
    };
    let s2 = sup.clone();
    let code = app.run_return(move |_, ev| {
        if let tauri::RunEvent::Exit = ev {
            s2.shutdown();
        }
    });
    sup.shutdown();
    // The control socket is removed with the server (process exit).
    let _ = cad_ipc::endpoint_path(&uictl::endpoint_name(std::process::id())).map(std::fs::remove_file);
    code
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64() {
        assert_eq!(
            super::b64_decode("data:image/png;base64,iVBORw0K").unwrap(),
            vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a]
        );
        assert_eq!(super::b64_decode("YWI=").unwrap(), b"ab");
        assert_eq!(super::b64_decode("YQ==").unwrap(), b"a");
    }
}
