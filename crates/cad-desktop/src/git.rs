//! Line markers against `HEAD` for the editor gutter (new, changed, deleted lines).
//!
//! The `HEAD` version of a file comes from the `git` command line (`git rev-parse
//! --show-toplevel`, `git ls-files`, `git show HEAD:<pfad>`), the diff against the editor
//! buffer is computed here with `similar` (Myers, by lines). Reasons for the command line
//! instead of `gix`/`git2`: it is one blob read per file and `HEAD` change (cached, see
//! [`GitBases`]), so speed does not matter; `git2` needs libgit2 (C build), `gix` adds well
//! over a hundred crates to the build of every binary — the `git` command is there wherever
//! a repository is used. Typing only re-runs the in-process diff; `HEAD` is checked again at most every 5 s.
//!
//! A file that is not in a repository, not tracked or not in `HEAD` (new, only staged) gets
//! no markers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use similar::{DiffOp, capture_diff_slices};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HunkKind {
    /// Lines `start..=end` are new.
    Added,
    /// Lines `start..=end` replace `old`.
    Modified,
    /// `old` was removed before line `start` (`start = lines + 1`: at the end); `end = start`.
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hunk {
    pub kind: HunkKind,
    /// 1-based lines of the buffer.
    pub start: u32,
    pub end: u32,
    /// Lines of `HEAD` (changed or deleted ones).
    pub old: Vec<String>,
}

fn split_lines(s: &str) -> Vec<&str> {
    let mut v: Vec<&str> = s.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    // A final newline does not start another line.
    if v.last() == Some(&"") {
        v.pop();
    }
    v
}

/// Hunks of `current` against `base`.
pub fn diff_hunks(base: &str, current: &str) -> Vec<Hunk> {
    let old = split_lines(base);
    let new = split_lines(current);
    let mut out = Vec::new();
    for op in capture_diff_slices(similar::Algorithm::Myers, &old, &new) {
        match op {
            DiffOp::Equal { .. } => {}
            DiffOp::Delete { old_index, old_len, new_index } => out.push(Hunk {
                kind: HunkKind::Deleted,
                start: new_index as u32 + 1,
                end: new_index as u32 + 1,
                old: old[old_index..old_index + old_len].iter().map(|s| s.to_string()).collect(),
            }),
            DiffOp::Insert { new_index, new_len, .. } => out.push(Hunk {
                kind: HunkKind::Added,
                start: new_index as u32 + 1,
                end: (new_index + new_len) as u32,
                old: Vec::new(),
            }),
            DiffOp::Replace { old_index, old_len, new_index, new_len } => out.push(Hunk {
                kind: HunkKind::Modified,
                start: new_index as u32 + 1,
                end: (new_index + new_len) as u32,
                old: old[old_index..old_index + old_len].iter().map(|s| s.to_string()).collect(),
            }),
        }
    }
    out
}

/// The `HEAD` state of a file.
#[derive(Debug, Clone, PartialEq)]
pub enum Base {
    /// Not in a repository, untracked, or not in `HEAD`.
    None,
    Head {
        repo: PathBuf,
        commit: String,
        text: String,
    },
}

fn git(dir: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).env("GIT_OPTIONAL_LOCKS", "0").output().ok()?;
    out.status.success().then_some(out.stdout)
}

fn trimmed(b: Vec<u8>) -> String {
    String::from_utf8_lossy(&b).trim().to_string()
}

/// Reads the `HEAD` version of `path` with the `git` command.
pub fn head_base(path: &Path) -> Base {
    let Ok(abs) = std::path::absolute(path) else { return Base::None };
    let Some(dir) = abs.parent() else { return Base::None };
    if !dir.is_dir() {
        return Base::None;
    }
    let Some(top) = git(dir, &["rev-parse", "--show-toplevel"]).map(trimmed) else { return Base::None };
    let Some(commit) = git(dir, &["rev-parse", "--verify", "-q", "HEAD"]).map(trimmed) else { return Base::None };
    let repo = PathBuf::from(&top);
    let canon_dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let canon_top = std::fs::canonicalize(&repo).unwrap_or(repo.clone());
    let Some(name) = abs.file_name() else { return Base::None };
    let Ok(rel) = canon_dir.join(name).strip_prefix(&canon_top).map(|r| r.to_string_lossy().replace('\\', "/")) else {
        return Base::None;
    };
    // Tracked and in HEAD?
    let Some(blob) = git(&repo, &["show", &format!("HEAD:{rel}")]) else { return Base::None };
    let Ok(text) = String::from_utf8(blob) else { return Base::None };
    Base::Head { repo, commit, text }
}

/// Cache of `HEAD` versions by file, refreshed when the commit changes or on request (save,
/// external change).
#[derive(Default)]
pub struct GitBases {
    /// Base and time of the last `HEAD` check.
    map: HashMap<PathBuf, (Base, std::time::Instant)>,
}

/// `HEAD` is checked again at most this often while typing.
const HEAD_CHECK: std::time::Duration = std::time::Duration::from_secs(5);

impl GitBases {
    /// Base of `path`; `refresh`: read it again (save, external change, focus).
    pub fn base(&mut self, path: &Path, refresh: bool) -> &Base {
        let k = crate::watcher::key(path);
        let stale = refresh
            || match self.map.get(&k) {
                None => true,
                Some((_, t)) if t.elapsed() < HEAD_CHECK => false,
                Some((Base::Head { repo, commit, .. }, _)) => {
                    git(repo, &["rev-parse", "--verify", "-q", "HEAD"]).map(trimmed).as_deref() != Some(commit)
                }
                // A file may become tracked: look again now and then.
                Some((Base::None, _)) => true,
            };
        if stale {
            let b = head_base(path);
            self.map.insert(k.clone(), (b, std::time::Instant::now()));
        } else if let Some(e) = self.map.get_mut(&k)
            && e.1.elapsed() >= HEAD_CHECK
        {
            // HEAD was checked and is unchanged.
            e.1 = std::time::Instant::now();
        }
        &self.map[&k].0
    }

    /// Hunks of the buffer `text` of `path` (empty without a base).
    pub fn hunks(&mut self, path: &Path, text: &str, refresh: bool) -> Option<Vec<Hunk>> {
        match self.base(path, refresh) {
            Base::None => None,
            Base::Head { text: base, .. } => Some(diff_hunks(base, text)),
        }
    }
}
