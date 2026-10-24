//! `cad usage [TOPIC…] [--all] [--list]`: the usage manual for agents and LLMs. The pages are
//! the Markdown files in `manuell/` (English), embedded at build time (`build.rs`); the number
//! prefix of a file only orders the pages (`03-render.md` → topic `render`).

use std::io::Write as _;
use std::process::ExitCode;

use clap::Args;

include!(concat!(env!("OUT_DIR"), "/manual.rs"));

#[derive(Args, Clone, Debug)]
pub(crate) struct UsageArgs {
    /// Topics to print (unique prefixes are enough), e.g. `render measure`. Without a topic:
    /// the overview and the list of topics.
    #[arg(value_name = "TOPIC")]
    pub topics: Vec<String>,
    /// The whole manual in one go (all pages in order).
    #[arg(long, conflicts_with_all = ["topics", "list"])]
    pub all: bool,
    /// Only the list of topics with their titles.
    #[arg(long, conflicts_with = "topics")]
    pub list: bool,
}

/// Title of a page: its first `# ` heading.
fn title(text: &str) -> &str {
    text.lines().find_map(|l| l.strip_prefix("# ")).unwrap_or("").trim()
}

fn list() -> String {
    let w = PAGES.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
    let mut s = String::from("Topics (`cad usage TOPIC…`, everything: `cad usage --all`):\n\n");
    for (name, text) in PAGES {
        s.push_str(&format!("  {name:<w$}  {}\n", title(text)));
    }
    s
}

/// The page of a topic: exact name, else a unique prefix.
fn find(topic: &str) -> Result<&'static (&'static str, &'static str), String> {
    let t = topic.to_ascii_lowercase();
    if let Some(p) = PAGES.iter().find(|(n, _)| *n == t) {
        return Ok(p);
    }
    let hits: Vec<_> = PAGES.iter().filter(|(n, _)| n.starts_with(&t)).collect();
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("cad usage: unknown topic `{topic}`.\n\n{}", list())),
        many => Err(format!(
            "cad usage: `{topic}` is ambiguous: {}",
            many.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// Text that `cad usage` prints for these arguments, or an error message (exit 2).
pub(crate) fn text(a: &UsageArgs) -> Result<String, String> {
    if a.list {
        return Ok(list());
    }
    let pages: Vec<&str> = if a.all {
        PAGES.iter().map(|(_, t)| *t).collect()
    } else if a.topics.is_empty() {
        let mut s = PAGES.first().map(|(_, t)| t.to_string()).unwrap_or_default();
        s.push('\n');
        s.push_str(&list());
        return Ok(s);
    } else {
        a.topics.iter().map(|t| find(t).map(|(_, text)| *text)).collect::<Result<_, _>>()?
    };
    Ok(pages.iter().map(|t| t.trim_end()).collect::<Vec<_>>().join("\n\n---\n\n") + "\n")
}

pub(crate) fn command(a: &UsageArgs) -> ExitCode {
    match text(a) {
        Ok(s) => {
            let mut o = std::io::stdout().lock();
            let _ = o.write_all(s.as_bytes());
            let _ = o.flush();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(topics: &[&str], all: bool, list: bool) -> UsageArgs {
        UsageArgs { topics: topics.iter().map(|s| s.to_string()).collect(), all, list }
    }

    #[test]
    fn pages_are_embedded() {
        assert!(PAGES.len() >= 5, "{}", PAGES.len());
        assert_eq!(PAGES[0].0, "overview");
        for (name, text) in PAGES {
            assert!(!title(text).is_empty(), "{name}: no `# ` title");
            assert!(name.chars().all(|c| c.is_ascii_lowercase() || c == '-'), "{name}");
        }
    }

    #[test]
    fn topics_and_prefixes() {
        let overview = text(&args(&[], false, false)).unwrap();
        assert!(overview.starts_with(PAGES[0].1.trim_start()) && overview.contains("Topics"), "{overview}");
        for (name, page) in PAGES {
            assert_eq!(text(&args(&[name], false, false)).unwrap(), format!("{}\n", page.trim_end()));
        }
        assert!(text(&args(&["gibt-es-nicht"], false, false)).unwrap_err().contains("unknown topic"));
        let all = text(&args(&[], true, false)).unwrap();
        assert!(PAGES.iter().all(|(_, p)| all.contains(p.trim_end())));
    }

    /// Every subcommand of `cad` is described in the manual (`cad <name>` appears somewhere).
    #[test]
    fn every_command_is_documented() {
        use clap::CommandFactory as _;
        let all = text(&args(&[], true, false)).unwrap();
        for sub in crate::Cli::command().get_subcommands().filter(|s| !s.is_hide_set()) {
            let name = sub.get_name();
            assert!(all.contains(&format!("cad {name}")), "`cad {name}` missing in manuell/");
        }
    }
}
