# Mitgelieferte Schriften

Alle Schriften stehen unter der SIL Open Font License 1.1 (Lizenztexte daneben). Sie werden
unverändert in `cad-font` eingebettet (`include_bytes!`) und in `text()` über Familie und Stil
gewählt (`font = "Inter:style=Bold"`).

| Datei | Familie / Stil | Quelle | Version | SHA-256 |
| --- | --- | --- | --- | --- |
| `JetBrainsMono-Regular.ttf` | JetBrains Mono / Regular | https://github.com/JetBrains/JetBrainsMono/releases/tag/v2.304 (`JetBrainsMono-2.304.zip`, `fonts/ttf/`) | 2.304 | `a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f` |
| `JetBrainsMono-Bold.ttf` | JetBrains Mono / Bold | ebenda | 2.304 | `5590990c82e097397517f275f430af4546e1c45cff408bde4255dad142479dcb` |
| `Inter-Regular.ttf` | Inter / Regular | https://github.com/rsms/inter/releases/tag/v4.1 (`Inter-4.1.zip`, `extras/ttf/`) | 4.1 | `40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82` |
| `Inter-Bold.ttf` | Inter / Bold | ebenda | 4.1 | `288316099b1e0a47a4716d159098005eef7c0066921f34e3200393dbdb01947f` |
| `AllertaStencil-Regular.ttf` | Allerta Stencil / Regular | https://github.com/google/fonts/tree/main/ofl/allertastencil (Upstream: https://github.com/librefonts/allerta) | Google Fonts, Stand 2026-10 | `8c504d1be20768941ab04251a389ad82d5782d647fa1c4029c811dc5bc2abd59` |

Lizenzen: `OFL-JetBrainsMono.txt` (Copyright 2020 The JetBrains Mono Project Authors),
`OFL-Inter.txt` (Copyright 2016 The Inter Project Authors), `OFL-AllertaStencil.txt`
(Copyright 2010 Matt McInerney, Reserved Font Name „Allerta“).

**Standardschrift.** `"cad stencil mono"` ist als künftige Standardschrift reserviert (eigene
Schablonenschrift aus Geraden und Bögen, Plan 17.4; folgt mit `stroke`). Bis dahin wählen der
Name und ein fehlendes `font` JetBrains Mono.
