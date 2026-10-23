//! File watcher for the open project (like VS Code): reports external changes of the watched
//! files, never the app's own saves.
//!
//! * Watches the parent directories (not the files): editors and `cad set` save atomically
//!   (temporary file + rename), which replaces the file's inode.
//! * Every watched file has a known content (hash of what the UI loaded or saved last). An
//!   event whose file content equals the known content is dropped — this is how own saves
//!   ([`FileWatcher::note_written`]) never come back as "changed".
//! * Events are debounced (50 ms). A file that is missing is checked again after 250 ms
//!   (atomic saves remove it briefly) before it is reported as [`WatchEvent::Removed`]; when
//!   it appears again it is reported as changed.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

#[derive(Debug, Clone, PartialEq)]
pub enum WatchEvent {
    /// The file has new content (written by someone else).
    Changed { path: PathBuf, text: String },
    /// The file was deleted or renamed away.
    Removed { path: PathBuf },
}

impl WatchEvent {
    pub fn path(&self) -> &Path {
        match self {
            WatchEvent::Changed { path, .. } | WatchEvent::Removed { path } => path,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Known {
    Hash(u64),
    Missing,
}

fn hash(bytes: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// Canonical key of a path: canonical directory + file name (the file may be missing).
pub fn key(path: &Path) -> PathBuf {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    match (abs.parent(), abs.file_name()) {
        (Some(d), Some(n)) => std::fs::canonicalize(d).unwrap_or_else(|_| d.to_path_buf()).join(n),
        _ => abs,
    }
}

struct Shared {
    /// Watched files (canonical key) → known content.
    files: HashMap<PathBuf, Known>,
    /// Original path as given by the UI, by key.
    names: HashMap<PathBuf, PathBuf>,
    dirs: HashSet<PathBuf>,
}

pub struct FileWatcher {
    shared: Arc<Mutex<Shared>>,
    watcher: Mutex<RecommendedWatcher>,
}

impl FileWatcher {
    /// Starts the watcher; `callback` gets every event (from a background thread).
    pub fn new(callback: impl Fn(WatchEvent) + Send + 'static) -> notify::Result<FileWatcher> {
        let (tx, rx) = channel::<PathBuf>();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                for p in ev.paths {
                    let _ = tx.send(p);
                }
            }
        })?;
        let shared =
            Arc::new(Mutex::new(Shared { files: HashMap::new(), names: HashMap::new(), dirs: HashSet::new() }));
        let sh = shared.clone();
        std::thread::Builder::new().name("cad-watch".into()).spawn(move || run(rx, sh, callback)).expect("thread");
        Ok(FileWatcher { shared, watcher: Mutex::new(watcher) })
    }

    /// Replaces the watched files; `text`: the content the UI has loaded (`None`: read it now).
    pub fn set_files(&self, files: &[(PathBuf, Option<String>)]) {
        let mut sh = self.shared.lock().unwrap();
        let mut w = self.watcher.lock().unwrap();
        sh.files.clear();
        sh.names.clear();
        let mut dirs = HashSet::new();
        for (p, text) in files {
            let k = key(p);
            let known = match text {
                Some(t) => Known::Hash(hash(t.as_bytes())),
                None => std::fs::read(&k).map(|b| Known::Hash(hash(&b))).unwrap_or(Known::Missing),
            };
            if let Some(d) = k.parent() {
                dirs.insert(d.to_path_buf());
            }
            sh.files.insert(k.clone(), known);
            sh.names.insert(k, p.clone());
        }
        for d in sh.dirs.difference(&dirs) {
            let _ = w.unwatch(d);
        }
        for d in dirs.difference(&sh.dirs.clone()) {
            let _ = w.watch(d, RecursiveMode::NonRecursive);
        }
        sh.dirs = dirs;
    }

    /// Adds one file to the watched set.
    pub fn add_file(&self, path: &Path, text: Option<&str>) {
        let mut all: Vec<(PathBuf, Option<String>)> = {
            let sh = self.shared.lock().unwrap();
            sh.names.values().map(|p| (p.clone(), None)).collect()
        };
        all.retain(|(p, _)| key(p) != key(path));
        // keep the known contents of the others
        let known: HashMap<PathBuf, Known> = self.shared.lock().unwrap().files.clone();
        all.push((path.to_path_buf(), text.map(String::from)));
        self.set_files(&all);
        let mut sh = self.shared.lock().unwrap();
        for (k, v) in known {
            if k != key(path)
                && let Some(e) = sh.files.get_mut(&k)
            {
                *e = v;
            }
        }
    }

    /// The app wrote (or loaded) `text` to `path`: events with this content are not reported.
    pub fn note_written(&self, path: &Path, text: &str) {
        let mut sh = self.shared.lock().unwrap();
        let k = key(path);
        if let Some(e) = sh.files.get_mut(&k) {
            *e = Known::Hash(hash(text.as_bytes()));
        }
    }

    /// Watched files (as given).
    pub fn watched(&self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = self.shared.lock().unwrap().names.values().cloned().collect();
        v.sort();
        v
    }
}

fn run(rx: Receiver<PathBuf>, sh: Arc<Mutex<Shared>>, cb: impl Fn(WatchEvent)) {
    let debounce = Duration::from_millis(50);
    let recheck = Duration::from_millis(250);
    let mut due: HashMap<PathBuf, Instant> = HashMap::new();
    let mut rechecking: HashSet<PathBuf> = HashSet::new();
    loop {
        let wait = due
            .values()
            .min()
            .map(|t| t.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(3600));
        match rx.recv_timeout(wait) {
            Ok(p) => {
                let k = key(&p);
                if sh.lock().unwrap().files.contains_key(&k) {
                    let t = Instant::now() + debounce;
                    due.entry(k).and_modify(|d| *d = (*d).max(t)).or_insert(t);
                }
                continue;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let now = Instant::now();
        let ready: Vec<PathBuf> = due.iter().filter(|(_, t)| **t <= now).map(|(k, _)| k.clone()).collect();
        for k in ready {
            due.remove(&k);
            let read = std::fs::read(&k);
            let mut s = sh.lock().unwrap();
            let Some(known) = s.files.get(&k).copied() else { continue };
            let name = s.names.get(&k).cloned().unwrap_or_else(|| k.clone());
            match read {
                Ok(bytes) => {
                    rechecking.remove(&k);
                    let h = hash(&bytes);
                    if known == Known::Hash(h) {
                        continue;
                    }
                    let Ok(text) = String::from_utf8(bytes) else { continue };
                    s.files.insert(k.clone(), Known::Hash(h));
                    drop(s);
                    cb(WatchEvent::Changed { path: name, text });
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if known == Known::Missing {
                        rechecking.remove(&k);
                        continue;
                    }
                    // Second look after a pause: atomic saves remove the file briefly.
                    if rechecking.insert(k.clone()) {
                        due.insert(k, now + recheck);
                        continue;
                    }
                    rechecking.remove(&k);
                    s.files.insert(k.clone(), Known::Missing);
                    drop(s);
                    cb(WatchEvent::Removed { path: name });
                }
                Err(_) => {}
            }
        }
    }
}
