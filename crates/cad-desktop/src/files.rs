//! Files and projects on the real file system: open a file (with everything it includes or
//! uses), open a folder as a project, save atomically, recently opened files.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use cad_eval::{FsResolver, Project, normalize_path, resolve_path};
use serde::{Deserialize, Serialize};

/// A project as the UI gets it: paths relative to `root` (`/` separators; `../x.scad` for
/// included files outside the root).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenedProject {
    /// Absolute directory the paths are relative to.
    pub root: PathBuf,
    /// Display name (file or folder name).
    pub name: String,
    /// Main file (relative path).
    pub main: String,
    /// File shown first.
    pub active: String,
    pub files: BTreeMap<String, String>,
    /// Opened as a folder (all `.scad` files of the folder belong to the project).
    pub folder: bool,
}

impl OpenedProject {
    /// Absolute path of a project file.
    pub fn abs(&self, rel: &str) -> PathBuf {
        abs_path(&self.root, rel)
    }
}

/// Absolute path of `rel` below `root` (`..` folded).
pub fn abs_path(root: &Path, rel: &str) -> PathBuf {
    let p = Path::new(rel);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    let joined = format!("{}/{}", root.to_string_lossy().replace('\\', "/"), rel);
    PathBuf::from(normalize_path(&joined))
}

pub fn read_text(path: &Path) -> io::Result<String> {
    let b = std::fs::read(path)?;
    String::from_utf8(b).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, format!("{}: kein UTF-8", path.display())))
}

/// Writes `text` atomically (temporary file in the same directory, then rename); keeps the
/// permissions of an existing file. Creates missing parent directories.
pub fn write_atomic(path: &Path, text: &str) -> io::Result<()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "datei".into());
    let tmp = dir.join(format!(".{name}.cad-{}.tmp", std::process::id()));
    std::fs::write(&tmp, text)?;
    if let Ok(md) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, md.permissions());
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Files the main file includes or uses (transitively), relative to `root`, with texts.
fn closure(root: &Path, main: &str, text: &str) -> BTreeMap<String, String> {
    let proj = Project::load(main, text, &FsResolver { root: root.to_path_buf() });
    proj.files.iter().filter(|f| f.path != main).map(|f| (f.path.clone(), f.text.clone())).collect()
}

/// Opens one file: the project is the file and everything it includes or uses.
pub fn open_file(path: &Path) -> io::Result<OpenedProject> {
    let path = std::path::absolute(path)?;
    let text = read_text(&path)?;
    let root = path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("/"));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "datei.scad".into());
    let mut files = closure(&root, &name, &text);
    // Files that do not exist are diagnostics of the engine, not project files.
    files.retain(|p, _| abs_path(&root, p).is_file());
    files.insert(name.clone(), text);
    Ok(OpenedProject { root, name: name.clone(), main: name.clone(), active: name, files, folder: false })
}

/// Directories skipped when scanning a folder.
fn skip_dir(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "target" | "dist")
}

/// `.scad` files below `dir` (relative paths), at most `limit`.
pub fn scan_scad(dir: &Path, limit: usize) -> io::Result<Vec<String>> {
    fn walk(base: &Path, dir: &Path, depth: usize, out: &mut Vec<String>, limit: usize) -> io::Result<()> {
        if depth > 8 || out.len() >= limit {
            return Ok(());
        }
        let mut entries: Vec<_> = std::fs::read_dir(dir)?.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            let p = e.path();
            let ft = e.file_type()?;
            if ft.is_dir() {
                if !skip_dir(&name) {
                    walk(base, &p, depth + 1, out, limit)?;
                }
            } else if ft.is_file() && name.ends_with(".scad") {
                if let Ok(rel) = p.strip_prefix(base) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
                if out.len() >= limit {
                    return Ok(());
                }
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, dir, 0, &mut out, limit)?;
    Ok(out)
}

/// `include <…>`/`use <…>` paths of a text (resolved relative to `from`).
pub fn references(from: &str, text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        let rest = t.strip_prefix("include").or_else(|| t.strip_prefix("use"));
        if let Some(r) = rest
            && let Some(r) = r.trim_start().strip_prefix('<')
            && let Some(end) = r.find('>')
        {
            out.push(resolve_path(from, &r[..end]));
        }
    }
    out
}

/// Main file of a folder: `hint` if given, else `main.scad`, else the first file (by path)
/// that no other file includes or uses.
pub fn choose_main(files: &BTreeMap<String, String>, hint: Option<&str>) -> Option<String> {
    if let Some(h) = hint.filter(|h| files.contains_key(*h)) {
        return Some(h.to_string());
    }
    if files.contains_key("main.scad") {
        return Some("main.scad".into());
    }
    let referenced: BTreeSet<String> = files.iter().flat_map(|(p, t)| references(p, t)).collect();
    files.keys().find(|p| !referenced.contains(*p)).or_else(|| files.keys().next()).cloned()
}

/// Opens a folder as a project (all `.scad` files; `main`: a file in it to use as main file).
pub fn open_folder(dir: &Path, main: Option<&Path>) -> io::Result<OpenedProject> {
    let root = std::path::absolute(dir)?;
    if !root.is_dir() {
        return Err(io::Error::new(io::ErrorKind::NotFound, format!("{} ist kein Ordner", root.display())));
    }
    let mut files = BTreeMap::new();
    for rel in scan_scad(&root, 500)? {
        if let Ok(t) = read_text(&root.join(&rel)) {
            files.insert(rel, t);
        }
    }
    let hint = main
        .and_then(|m| std::path::absolute(m).ok())
        .and_then(|m| m.strip_prefix(&root).ok().map(|r| r.to_string_lossy().replace('\\', "/")));
    let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Projekt".into());
    let Some(main) = choose_main(&files, hint.as_deref()) else {
        // An empty folder: start with a new main file.
        let main = "main.scad".to_string();
        files.insert(main.clone(), String::new());
        return Ok(OpenedProject { root, name, main: main.clone(), active: main, files, folder: true });
    };
    // Included files outside the folder.
    let extra = closure(&root, &main, &files[&main]);
    for (p, t) in extra {
        if !files.contains_key(&p) && abs_path(&root, &p).is_file() {
            files.insert(p, t);
        }
    }
    Ok(OpenedProject { root, name, main: main.clone(), active: main, files, folder: true })
}

/// Opens a path given on the command line: a folder, or a file (a file inside a folder that
/// is opened as project is not detected: files open as single-file projects).
pub fn open_path(path: &Path) -> io::Result<OpenedProject> {
    if path.is_dir() { open_folder(path, None) } else { open_file(path) }
}

// ------------------------------------------------------------------------------------------
// recently opened
// ------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentEntry {
    pub path: PathBuf,
    pub folder: bool,
}

/// Recently opened files and folders, newest first, kept in a JSON file.
pub struct Recent {
    pub file: PathBuf,
    pub max: usize,
}

impl Recent {
    pub fn new(file: PathBuf) -> Recent {
        Recent { file, max: 12 }
    }

    pub fn load(&self) -> Vec<RecentEntry> {
        std::fs::read(&self.file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    /// Adds an entry (moves it to the front); missing paths are dropped.
    pub fn add(&self, path: &Path, folder: bool) -> Vec<RecentEntry> {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let mut l = self.load();
        l.retain(|e| e.path != path && e.path.exists());
        l.insert(0, RecentEntry { path, folder });
        l.truncate(self.max);
        if let Ok(j) = serde_json::to_string_pretty(&l) {
            let _ = write_atomic(&self.file, &j);
        }
        l
    }
}
