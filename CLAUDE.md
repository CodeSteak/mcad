# CLAUDE.md

## Keep the usage manual (`manuell/`) in step with every feature

`mcad usage` prints the manual for agents/LLMs: the Markdown files in `manuell/` (English),
compiled into the binary by `crates/cad-cli/build.rs`. The number in the file name only sorts
(`04-measure.md` → `mcad usage measure`); the first `# ` line is the title.

**Every change a user of the CLI or the language can see updates the matching page in
`manuell/` in the same commit**: new or changed commands, options, defaults, JSON fields, exit
codes, language constructs (builtins, parameters, zones, sketches, colours), new error messages
with their typical cause (`17-troubleshooting.md`), new builtins in `20-quickref.md`. New topic
→ new numbered file.

Manual style: English, short and precise, tables instead of prose, nothing obvious. Run
examples with `mcad` first (example model `box.mcad` is in `10-language.md`).

Tests: `cargo test -p cad-cli usage` checks that every page has a title and every visible
command (`mcad <name>`) appears in the manual.

## Texts

- Everything in the repository is English: docs, comments, README, reports, manual.
- `mcad --help` and the help texts of all commands/options (doc comments of the clap structs in
  `crates/cad-cli/src/`) are English only.
- Other user-visible output (diagnostics, results, render labels) exists in English and German:
  `cad_i18n::tr!("German …", "English …", args)` / `trs!`, never a plain literal. Language:
  `--lang`, `MCAD_LANG`, locale; default English. JSON keys are English.
- Web UI: texts only through the i18n dictionaries `web/src/i18n/` (English and German), never
  hard-coded.

## Building

Without `third_party/smlib`: set `SMLIB_SRC` and `SMLIB_PREBUILT` (see `Makefile`,
`smlib/fetch.sh`).
