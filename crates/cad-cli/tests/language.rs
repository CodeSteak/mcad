//! Output language: `--lang en|de`, `MCAD_LANG`, the locale (`LANG`, …); default English.
//! The same commands in English and German (the other test files run in German).

use assert_cmd::Command;
use serde_json::Value;

fn data(name: &str) -> String {
    format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// `mcad` without any language setting from outside.
fn mcad() -> Command {
    let mut c = Command::cargo_bin("mcad").unwrap();
    c.env("MCAD_NO_DAEMON", "1");
    for k in ["MCAD_LANG", "LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        c.env_remove(k);
    }
    c
}

fn out(c: &mut Command) -> (String, String, Option<i32>) {
    let o = c.output().unwrap();
    (String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned(), o.status.code())
}

#[test]
fn check_in_english_and_german() {
    let f = data("fillet_too_large.mcad");
    let (en, _, code) = out(mcad().args(["check", &f, "--lang", "en"]));
    assert_eq!(code, Some(1));
    assert!(en.contains(": error (kernel): "), "{en}");
    assert!(en.contains(": 1 errors, 0 warnings"), "{en}");
    let (de, _, code) = out(mcad().args(["check", &f, "--lang", "de"]));
    assert_eq!(code, Some(1));
    assert!(de.contains(": Fehler (Kernel): "), "{de}");
    assert!(de.contains(": 1 Fehler, 0 Warnungen"), "{de}");
    // JSON: keys and machine values stay, the message follows the language
    let j = |lang: &str| -> Value {
        let (s, _, _) = out(mcad().args(["check", &f, "--json", "--lang", lang]));
        serde_json::from_str(&s).unwrap()
    };
    let (je, jd) = (j("en"), j("de"));
    assert_eq!(je["diagnostics"][0]["severity"], "error");
    assert_eq!(je["diagnostics"][0]["source"], jd["diagnostics"][0]["source"]);
    assert_ne!(je["diagnostics"][0]["message"], jd["diagnostics"][0]["message"]);
}

#[test]
fn default_and_environment() {
    let f = data("fillet_too_large.mcad");
    // no setting: English
    let (s, _, _) = out(mcad().args(["check", &f]));
    assert!(s.contains(" errors, "), "{s}");
    // German locale
    let (s, _, _) = out(mcad().env("LANG", "de_DE.UTF-8").args(["check", &f]));
    assert!(s.contains(" Fehler, "), "{s}");
    // MCAD_LANG before the locale, --lang before MCAD_LANG
    let (s, _, _) = out(mcad().env("LANG", "de_DE.UTF-8").env("MCAD_LANG", "en").args(["check", &f]));
    assert!(s.contains(" errors, "), "{s}");
    let (s, _, _) = out(mcad().env("MCAD_LANG", "en").args(["--lang", "de", "check", &f]));
    assert!(s.contains(" Fehler, "), "{s}");
    // an unknown locale: English
    let (s, _, _) = out(mcad().env("LANG", "fr_FR.UTF-8").args(["check", &f]));
    assert!(s.contains(" errors, "), "{s}");
    // invalid --lang: usage error of clap
    let (_, e, code) = out(mcad().args(["check", &f, "--lang", "fr"]));
    assert_eq!(code, Some(2), "{e}");
    // usage errors follow the language too
    let (_, e, code) = out(mcad().args(["export", &f, "-o", "x.xyz", "--lang", "en"]));
    assert_eq!(code, Some(2));
    assert!(e.contains("unknown export format: xyz"), "{e}");
    let (_, e, _) = out(mcad().args(["export", &f, "-o", "x.xyz", "--lang", "de"]));
    assert!(e.contains("unbekanntes Exportformat: xyz"), "{e}");
}

#[test]
fn measure_in_english_and_german() {
    let f = data("plate.mcad");
    let args = |lang: &'static str| {
        ["measure", f.as_str(), "--volume", "--distance", "vertex@[0,0,0]", "vertex@[1000,1000,1000]", "--lang", lang]
    };
    let (en, e, code) = out(mcad().args(args("en")));
    assert_eq!(code, Some(0), "{e}");
    assert!(en.contains("Distance vertex "), "{en}");
    assert!(en.contains("Volume all: "), "{en}");
    let (de, _, _) = out(mcad().args(args("de")));
    assert!(de.contains("Abstand Eckpunkt "), "{de}");
    assert!(de.contains("Volumen all: "), "{de}");
}

#[test]
fn outline_in_english_and_german() {
    let f = data("housing.mcad");
    let (en, _, _) = out(mcad().args(["outline", &f, "--lang", "en"]));
    assert!(en.contains("Modules:\n"), "{en}");
    assert!(en.contains(": number"), "{en}");
    assert!(en.contains(" errors, "), "{en}");
    let (de, _, _) = out(mcad().args(["outline", &f, "--lang", "de"]));
    assert!(de.contains("Module:\n"), "{de}");
    assert!(de.contains(": Zahl"), "{de}");
    // the JSON keeps machine values (type names) in both languages
    let j = |lang: &str| -> Value {
        let (s, _, _) = out(mcad().args(["outline", &f, "--json", "--lang", lang]));
        serde_json::from_str(&s).unwrap()
    };
    assert_eq!(j("en")["modules"], j("de")["modules"]);
}

#[test]
fn eval_and_inspect_in_english() {
    let f = data("fillet_zones.mcad");
    let (en, _, _) = out(mcad().args(["inspect", &f, "--line", "12", "--lang", "en"]));
    assert!(en.contains("line 12: "), "{en}");
    assert!(en.contains("hint: 2 edges"), "{en}");
    let (en, _, _) = out(mcad().args(["eval", &f, "--lang", "en"]));
    assert!(en.contains("Tree:\n") && en.contains("Statistics: "), "{en}");
}
