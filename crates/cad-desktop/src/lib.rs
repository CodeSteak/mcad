//! Desktop backend without Tauri (testable headless): the engine process and its supervisor
//! with watchdog ([`engine`]), the binary wire format of engine answers ([`wire`]), files and
//! projects on disk ([`files`]), the file watcher ([`watcher`]), git line markers ([`git`])
//! and the control protocol for `mcad ui …` ([`uictl`]). The Tauri app (`cad-app`) only wires
//! these to commands and events.

pub mod engine;
pub mod files;
pub mod git;
pub mod uictl;
pub mod watcher;
pub mod wire;
