//! Settings of the CLI daemon: defaults < configuration file < environment < arguments of
//! `cad daemon serve`.
//!
//! File: `$CAD_CONFIG`, else `$XDG_CONFIG_HOME/controller-cad/config.toml`, else
//! `~/.config/controller-cad/config.toml`:
//!
//! ```toml
//! [daemon]
//! enabled = true          # false: never use the daemon (like CAD_NO_DAEMON=1)
//! cache_mb = 512          # estimated size of node results + meshes
//! max_projects = 8        # projects whose results the cache keeps
//! idle_minutes = 15       # ends after this long without a request
//! watchdog_secs = 60      # a command running longer: the daemon ends, the CLI computes itself
//! rss_factor = 1.5        # resident memory above start + factor × cache_mb: cache cleared
//! cache_entries = 16384   # maximum number of cache entries
//! ```
//!
//! `daemon = false` at the top level (instead of the table) also switches it off.
//! Environment: `CAD_NO_DAEMON=1`, `CAD_DAEMON_CACHE_MB`, `CAD_DAEMON_MAX_PROJECTS`,
//! `CAD_DAEMON_IDLE_SECS` (or `CAD_DAEMON_IDLE_MINUTES`), `CAD_DAEMON_WATCHDOG_SECS`,
//! `CAD_DAEMON_RSS_FACTOR`, `CAD_DAEMON_CACHE_ENTRIES`.

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DaemonConfig {
    pub enabled: bool,
    /// Why the daemon is off (`enabled == false`).
    pub disabled_by: Option<String>,
    pub cache_mb: u64,
    pub max_projects: usize,
    #[serde(serialize_with = "secs")]
    pub idle: Duration,
    #[serde(serialize_with = "secs")]
    pub watchdog: Duration,
    pub rss_factor: f64,
    pub cache_entries: usize,
    /// Configuration file read (if it exists).
    pub file: Option<PathBuf>,
    /// Problems with the file or the environment (ignored values).
    pub warnings: Vec<String>,
}

fn secs<S: serde::Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64(d.as_secs_f64())
}

impl Default for DaemonConfig {
    fn default() -> Self {
        DaemonConfig {
            enabled: true,
            disabled_by: None,
            cache_mb: 512,
            max_projects: 8,
            idle: Duration::from_secs(15 * 60),
            watchdog: Duration::from_secs(60),
            rss_factor: 1.5,
            cache_entries: 16384,
            file: None,
            warnings: Vec::new(),
        }
    }
}

/// Path of the configuration file.
pub fn config_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("CAD_CONFIG").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    if let Some(d) = std::env::var_os("XDG_CONFIG_HOME").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(d).join("controller-cad/config.toml"));
    }
    std::env::var_os("HOME")
        .filter(|p| !p.is_empty())
        .map(|h| PathBuf::from(h).join(".config/controller-cad/config.toml"))
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

impl DaemonConfig {
    /// Defaults, file, environment.
    pub fn load() -> DaemonConfig {
        let mut c = DaemonConfig::default();
        if let Some(path) = config_path()
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            c.apply_file(&text, &path.display().to_string());
            c.file = Some(path);
        }
        c.apply_env();
        c
    }

    fn apply_file(&mut self, text: &str, name: &str) {
        let v: toml::Value = match toml::from_str(text) {
            Ok(v) => v,
            Err(e) => {
                self.warnings.push(format!("{name}: {e}"));
                return;
            }
        };
        let t = match v.get("daemon") {
            None => return,
            Some(toml::Value::Boolean(b)) => {
                self.set_enabled(*b, &format!("{name}: daemon = false"));
                return;
            }
            Some(toml::Value::Table(t)) => t,
            Some(_) => {
                self.warnings.push(format!("{name}: `daemon` muss eine Tabelle oder true/false sein"));
                return;
            }
        };
        for (k, val) in t {
            let num = val.as_float().or_else(|| val.as_integer().map(|i| i as f64));
            let ok = match (k.as_str(), num) {
                ("enabled", _) => match val.as_bool() {
                    Some(b) => {
                        self.set_enabled(b, &format!("{name}: [daemon] enabled = false"));
                        true
                    }
                    None => false,
                },
                ("cache_mb", Some(n)) => self.set_cache_mb(n),
                ("max_projects", Some(n)) if n >= 1.0 => {
                    self.max_projects = n as usize;
                    true
                }
                ("idle_minutes", Some(n)) => self.set_idle(n * 60.0),
                ("idle_secs", Some(n)) => self.set_idle(n),
                ("watchdog_secs", Some(n)) => self.set_watchdog(n),
                ("rss_factor", Some(n)) if n >= 1.0 => {
                    self.rss_factor = n;
                    true
                }
                ("cache_entries", Some(n)) if n >= 1.0 => {
                    self.cache_entries = n as usize;
                    true
                }
                _ => false,
            };
            if !ok {
                self.warnings.push(format!("{name}: [daemon] {k} = {val} ignoriert"));
            }
        }
    }

    fn apply_env(&mut self) {
        if let Some(v) = env("CAD_NO_DAEMON")
            && v != "0"
            && !v.eq_ignore_ascii_case("false")
        {
            self.set_enabled(false, "CAD_NO_DAEMON");
        }
        let mut num = |name: &str, f: &mut dyn FnMut(&mut Self, f64) -> bool| {
            if let Some(v) = env(name) {
                let ok = v.parse::<f64>().ok().is_some_and(|n| f(self, n));
                if !ok {
                    self.warnings.push(format!("{name}={v} ignoriert"));
                }
            }
        };
        num("CAD_DAEMON_CACHE_MB", &mut |c, n| c.set_cache_mb(n));
        num("CAD_DAEMON_MAX_PROJECTS", &mut |c, n| {
            c.max_projects = n.max(1.0) as usize;
            n >= 1.0
        });
        num("CAD_DAEMON_IDLE_MINUTES", &mut |c, n| c.set_idle(n * 60.0));
        num("CAD_DAEMON_IDLE_SECS", &mut |c, n| c.set_idle(n));
        num("CAD_DAEMON_WATCHDOG_SECS", &mut |c, n| c.set_watchdog(n));
        num("CAD_DAEMON_RSS_FACTOR", &mut |c, n| {
            c.rss_factor = n.max(1.0);
            n >= 1.0
        });
        num("CAD_DAEMON_CACHE_ENTRIES", &mut |c, n| {
            c.cache_entries = n.max(1.0) as usize;
            n >= 1.0
        });
    }

    fn set_enabled(&mut self, on: bool, by: &str) {
        self.enabled = on;
        self.disabled_by = (!on).then(|| by.to_string());
    }

    pub fn set_cache_mb(&mut self, n: f64) -> bool {
        if !(n.is_finite() && n >= 1.0) {
            return false;
        }
        self.cache_mb = n as u64;
        true
    }

    pub fn set_idle(&mut self, secs: f64) -> bool {
        if !(secs.is_finite() && secs > 0.0) {
            return false;
        }
        self.idle = Duration::from_secs_f64(secs);
        true
    }

    pub fn set_watchdog(&mut self, secs: f64) -> bool {
        if !(secs.is_finite() && secs > 0.0) {
            return false;
        }
        self.watchdog = Duration::from_secs_f64(secs);
        true
    }

    pub fn cache_bytes(&self) -> usize {
        (self.cache_mb as usize).saturating_mul(1 << 20)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_values_and_errors() {
        let mut c = DaemonConfig::default();
        c.apply_file("[daemon]\ncache_mb = 64\nidle_minutes = 0.5\nmax_projects = 2\nfoo = 1\n", "c.toml");
        assert_eq!(c.cache_mb, 64);
        assert_eq!(c.idle, Duration::from_secs(30));
        assert_eq!(c.max_projects, 2);
        assert!(c.enabled);
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);

        let mut c = DaemonConfig::default();
        c.apply_file("daemon = false\n", "c.toml");
        assert!(!c.enabled);
        let mut c = DaemonConfig::default();
        c.apply_file("[daemon]\nenabled = false\n", "c.toml");
        assert!(!c.enabled && c.disabled_by.is_some());
        let mut c = DaemonConfig::default();
        c.apply_file("[daemon\n", "c.toml");
        assert!(c.enabled && c.warnings.len() == 1);
    }
}
