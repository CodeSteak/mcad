//! Language of user-visible texts (diagnostics, CLI results): English or German.
//!
//! Both texts sit at the call site, so format arguments are checked for both:
//!
//! ```
//! use cad_i18n::{tr, trs, Lang};
//! let n = 4;
//! let _g = cad_i18n::scoped(Lang::De);
//! assert_eq!(tr!("{n} Kanten", "{n} edges"), "4 Kanten");
//! assert_eq!(trs!("Fehler", "error"), "Fehler");
//! ```
//!
//! The language is per thread (the daemon serves each connection in its own thread); the
//! default is English. Entry points set it once: the CLI from `--lang`/`MCAD_LANG`/locale,
//! the daemon per request, the UI engine from the UI setting.

use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Lang {
    #[default]
    En,
    De,
}

impl Lang {
    /// `"en"`/`"de"` (also `de_DE.UTF-8`, `de-AT`, `german` …); anything else is `None`.
    pub fn parse(s: &str) -> Option<Lang> {
        let s = s.trim().to_ascii_lowercase();
        if s.starts_with("de") || s == "german" || s == "deutsch" {
            Some(Lang::De)
        } else if s.starts_with("en") || s == "english" || s == "c" || s == "posix" {
            Some(Lang::En)
        } else {
            None
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::De => "de",
        }
    }

    /// From the environment: `MCAD_LANG`, then `LC_ALL`, `LC_MESSAGES`, `LANG`
    /// (first non-empty; German locale → German, else English).
    pub fn from_env() -> Lang {
        for k in ["MCAD_LANG", "LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(v) = std::env::var(k)
                && !v.trim().is_empty()
            {
                return Lang::parse(&v).unwrap_or(Lang::En);
            }
        }
        Lang::En
    }
}

thread_local! {
    static LANG: Cell<Lang> = const { Cell::new(Lang::En) };
}

/// Language of the current thread.
pub fn lang() -> Lang {
    LANG.with(|l| l.get())
}

/// `true` if the current thread speaks German.
pub fn is_de() -> bool {
    lang() == Lang::De
}

/// Sets the language of the current thread.
pub fn set_lang(l: Lang) {
    LANG.with(|c| c.set(l));
}

/// Sets the language until the guard is dropped (tests, one daemon request).
#[must_use]
pub fn scoped(l: Lang) -> Scoped {
    let prev = lang();
    set_lang(l);
    Scoped(prev)
}

pub struct Scoped(Lang);

impl Drop for Scoped {
    fn drop(&mut self) {
        set_lang(self.0);
    }
}

/// `format!` with a German and an English template (same arguments).
#[macro_export]
macro_rules! tr {
    ($de:literal, $en:literal $(,)?) => {
        if $crate::is_de() { format!($de) } else { format!($en) }
    };
    ($de:literal, $en:literal, $($arg:tt)+) => {
        if $crate::is_de() { format!($de, $($arg)+) } else { format!($en, $($arg)+) }
    };
}

/// A fixed text in both languages (`&'static str`).
#[macro_export]
macro_rules! trs {
    ($de:literal, $en:literal $(,)?) => {
        if $crate::is_de() { $de } else { $en }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_english_scoped_german() {
        assert_eq!(lang(), Lang::En);
        let x = 2.5;
        {
            let _g = scoped(Lang::De);
            assert_eq!(tr!("Radius {x} mm", "radius {x} mm"), "Radius 2.5 mm");
            assert_eq!(tr!("{} von {}", "{} of {}", 1, 2), "1 von 2");
        }
        assert_eq!(tr!("Radius {x} mm", "radius {x} mm"), "radius 2.5 mm");
        assert_eq!(trs!("Fehler", "error"), "error");
    }

    #[test]
    fn parse() {
        assert_eq!(Lang::parse("de_DE.UTF-8"), Some(Lang::De));
        assert_eq!(Lang::parse("en_US.UTF-8"), Some(Lang::En));
        assert_eq!(Lang::parse("C"), Some(Lang::En));
        assert_eq!(Lang::parse("fr_FR"), None);
    }
}
