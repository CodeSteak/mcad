# CLAUDE.md

## Usage-Manual (`manuell/`) bei jedem Feature mitziehen

`mcad usage` gibt das Manual für Agenten/LLMs aus: die Markdown-Dateien in `manuell/`
(Englisch), per `crates/cad-cli/build.rs` ins Binary kompiliert. Die Nummer im Dateinamen
ordnet nur (`04-measure.md` → `mcad usage measure`), die erste `# `-Zeile ist der Titel.

**Jede Änderung, die ein Nutzer der CLI oder der Sprache sieht, aktualisiert im selben Commit
die passende Seite in `manuell/`**: neue oder geänderte Befehle, Optionen, Standardwerte,
JSON-Felder, Exit-Codes, Sprachkonstrukte (Builtins, Parameter, Zonen, Skizzen, Farben),
neue Fehlermeldungen mit typischer Ursache (`17-troubleshooting.md`). Neues Thema → neue
Datei mit Nummer.

Stil des Manuals: Englisch, kurz und präzise, Tabellen statt Prosa, nichts Offensichtliches
erklären. Beispiele vorher mit `mcad` ausführen (Beispielmodell `box.mcad` steht in
`10-language.md`).

Tests: `cargo test -p cad-cli usage` prüft, dass jede Seite einen Titel hat und jeder
sichtbare Befehl (`mcad <name>`) im Manual vorkommt.

## CLI-Texte

- `mcad --help` und die Hilfetexte aller Befehle/Optionen (Doc-Kommentare der clap-Structs in
  `crates/cad-cli/src/`) sind Englisch.
- Die übrige Textausgabe der CLI (Diagnosen, Ergebnisse) ist Deutsch; JSON-Schlüssel sind
  Englisch. Neue deutsche Begriffe in der Ausgabe → Glossar in `17-troubleshooting.md`.
- `README.md` und die Testberichte (`docs/reports/`) sind Englisch; übrige Doku im Repo (Plan,
  `docs/development.md`, Skill) bleibt Deutsch.
- Web-UI: Texte nur über die i18n-Wörterbücher (Englisch und Deutsch), nie hart codiert.

## Bauen

Ohne `third_party/smlib`: `SMLIB_SRC` und `SMLIB_PREBUILT` setzen (siehe `Makefile`,
`smlib/fetch.sh`).
