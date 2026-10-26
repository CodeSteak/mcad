//! `mcad` without a command: start the desktop UI (feature `gui`), optionally with a file or a
//! folder. Without a display or without the feature: a message with the headless commands and
//! exit code 2.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cad_i18n::{tr, trs};

/// What `mcad [PFAD]` does.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Start the UI with this file/folder (or empty).
    Gui(Option<PathBuf>),
    /// The build has no UI.
    NotBuilt,
    /// No graphical session (Linux/BSD: neither `DISPLAY` nor `WAYLAND_DISPLAY`).
    NoDisplay,
    /// The path does not exist.
    Missing(PathBuf),
}

/// A graphical session is available. macOS and Windows always have one.
pub fn has_display(env: &dyn Fn(&str) -> Option<String>) -> bool {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        return true;
    }
    ["DISPLAY", "WAYLAND_DISPLAY"].iter().any(|v| env(v).is_some_and(|s| !s.is_empty()))
}

pub fn decide(target: Option<&Path>, env: &dyn Fn(&str) -> Option<String>, gui_built: bool) -> Decision {
    if let Some(p) = target
        && !p.exists()
    {
        return Decision::Missing(p.to_path_buf());
    }
    if !gui_built {
        return Decision::NotBuilt;
    }
    if !has_display(env) {
        return Decision::NoDisplay;
    }
    Decision::Gui(target.map(Path::to_path_buf))
}

fn headless() -> &'static str {
    trs!(
        "Ohne UI (headless) arbeiten die Befehle:\n  \
mcad check DATEI · mcad outline DATEI · mcad render DATEI -o bild.png --views all · mcad inspect DATEI --line N\n  \
mcad measure DATEI … · mcad watch DATEI --json --render bild.png · mcad set DATEI --param m.p=wert · mcad export DATEI -o teil.step\n\
Hilfe: mcad --help",
        "Without the UI (headless) the commands work:\n  \
mcad check FILE · mcad outline FILE · mcad render FILE -o image.png --views all · mcad inspect FILE --line N\n  \
mcad measure FILE … · mcad watch FILE --json --render image.png · mcad set FILE --param m.p=value · mcad export FILE -o part.step\n\
Help: mcad --help"
    )
}

/// Message (stderr) for a decision that does not start the UI.
pub fn message(d: &Decision, target: Option<&Path>) -> String {
    let what = target.map(|p| tr!(" für {}", " for {}", p.display())).unwrap_or_default();
    let headless = headless();
    match d {
        Decision::Gui(_) => String::new(),
        Decision::NotBuilt => tr!(
            "mcad: UI nicht in diesem Build (ohne Feature `gui` gebaut){what}.\n\
             Desktop-UI: cargo build --release -p cad-cli (Standard-Features, Linux: WebKitGTK).\n{headless}",
            "mcad: no UI in this build (built without the feature `gui`){what}.\n\
             Desktop UI: cargo build --release -p cad-cli (default features, Linux: WebKitGTK).\n{headless}"
        ),
        Decision::NoDisplay => tr!(
            "mcad: keine grafische Sitzung (weder DISPLAY noch WAYLAND_DISPLAY gesetzt), die Desktop-UI{what} startet nicht.\n{headless}",
            "mcad: no graphical session (neither DISPLAY nor WAYLAND_DISPLAY set), the desktop UI{what} does not start.\n{headless}"
        ),
        Decision::Missing(p) => tr!(
            "mcad: {} gibt es nicht (Datei .mcad/.scad oder Ordner erwartet).\n{headless}",
            "mcad: {} does not exist (expected a .mcad/.scad file or a folder).\n{headless}",
            p.display()
        ),
    }
}

/// `mcad [PFAD]`.
pub fn run(target: Option<&Path>) -> ExitCode {
    let env = |k: &str| std::env::var(k).ok();
    let d = decide(target, &env, cfg!(feature = "gui"));
    match d {
        Decision::Gui(open) => start(open),
        other => {
            eprintln!("{}", message(&other, target));
            ExitCode::from(2)
        }
    }
}

#[cfg(feature = "gui")]
fn start(open: Option<PathBuf>) -> ExitCode {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("mcad"));
    let code = cad_app::run(cad_app::LaunchOptions {
        open,
        engine_program: exe,
        engine_args: vec!["engine-host".into()],
        watchdog: cad_desktop::engine::WATCHDOG,
    });
    ExitCode::from(code.clamp(0, 255) as u8)
}

#[cfg(not(feature = "gui"))]
fn start(_open: Option<PathBuf>) -> ExitCode {
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(vars: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn decisions() {
        let here = Path::new(".");
        if !cfg!(any(target_os = "macos", target_os = "windows")) {
            assert_eq!(decide(None, &env_of(&[]), true), Decision::NoDisplay);
            assert_eq!(decide(None, &env_of(&[("DISPLAY", "")]), true), Decision::NoDisplay);
            assert_eq!(
                decide(Some(here), &env_of(&[("WAYLAND_DISPLAY", "wayland-0")]), true),
                Decision::Gui(Some(here.into()))
            );
        }
        assert_eq!(decide(None, &env_of(&[("DISPLAY", ":0")]), true), Decision::Gui(None));
        assert_eq!(decide(None, &env_of(&[("DISPLAY", ":0")]), false), Decision::NotBuilt);
        let missing = Path::new("gibt-es-nicht.scad");
        assert_eq!(decide(Some(missing), &env_of(&[("DISPLAY", ":0")]), true), Decision::Missing(missing.into()));
        let m = message(&Decision::NoDisplay, Some(Path::new("a.scad")));
        assert!(m.contains("DISPLAY") && m.contains("mcad check") && m.contains("a.scad"), "{m}");
    }
}
