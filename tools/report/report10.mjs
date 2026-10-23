#!/usr/bin/env node
// Test report 10 ("CLI-Daemon"): collects the results of the test runs and writes
//
//   docs/reports/10-cli-daemon/report.html
//
// Inputs (gitignored, reports/tmp/report10/):
//   timings.json      python3 tools/report/report10-measure.py --cad target/release/cad
//   cache_size.jsonl  target/release/examples/cache_size examples/spec/*.scad \
//                       examples/projekt-gehaeuse/gehaeuse.scad examples/openscad-compat/*.scad
//   cargo.txt         cargo test --workspace --no-fail-fast
//   daemon-tests.txt  cargo test -p cad-cli --test daemon -- --nocapture --test-threads 1
//   clippy.txt        cargo clippy --workspace --all-targets -- -D warnings
//   fmt.txt           cargo fmt --all --check (empty = ok)
//
//   node tools/report/report10.mjs
//   node tools/report/print-pdf.mjs docs/reports/10-cli-daemon/report.html docs/reports/10-cli-daemon/report.pdf "Testbericht 10: CLI-Daemon"

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report10");
const out = join(root, "docs/reports/10-cli-daemon");
mkdirSync(out, { recursive: true });

const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const readText = (f) => (existsSync(f) ? readFileSync(f, "utf8") : null);
const readJson = (f) => (existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : null);
const git = (...a) => {
  try {
    return execFileSync("git", a, { cwd: root, encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
};
const de = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("de-DE", { minimumFractionDigits: d, maximumFractionDigits: d }));
const mib = (b, d = 1) => de(b / 1048576, d);

const timings = readJson(join(input, "timings.json"));
const sizes = (readText(join(input, "cache_size.jsonl")) ?? "").split("\n").filter(Boolean).map((l) => JSON.parse(l));
const sizeRows = sizes.filter((r) => !r.summary);
const sizeSum = sizes.find((r) => r.summary);
const cargo = readText(join(input, "cargo.txt"));
const daemonTests = readText(join(input, "daemon-tests.txt"));
const clippy = readText(join(input, "clippy.txt"));
const fmt = readText(join(input, "fmt.txt"));

const cargoSum = (() => {
  if (!cargo) return null;
  let ok = 0;
  let failed = 0;
  let ignored = 0;
  for (const m of cargo.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored/g)) {
    ok += +m[1];
    failed += +m[2];
    ignored += +m[3];
  }
  return { ok, failed, ignored };
})();
const testsOf = (text, file) =>
  text
    ? text
        .split(/Running /)
        .filter((s) => s.split("\n")[0].includes(file))
        .flatMap((s) => [...s.matchAll(/^test (\S+) \.\.\. (ok|FAILED|ignored)$/gm)].map((m) => ({ name: m[1], res: m[2] })))
    : [];
const daemonRes = testsOf(cargo, "tests/daemon.rs");
const ipcRes = testsOf(cargo, "tests/ipc.rs").filter((t) => t.name.includes("lock"));
const unitRes = (cargo ? [...cargo.matchAll(/^test ((?:config|cache)::tests::\S+) \.\.\. (ok|FAILED)$/gm)] : []).map((m) => ({ name: m[1], res: m[2] }));
const daemonTimes = daemonTests ? [...daemonTests.matchAll(/first ([\d.]+)(ms|s), second ([\d.]+)(ms|s)/g)].map((m) => ({ first: +m[1] * (m[2] === "s" ? 1000 : 1), second: +m[3] * (m[4] === "s" ? 1000 : 1) }))[0] : null;

const date = new Date().toLocaleDateString("de-DE", { day: "2-digit", month: "2-digit", year: "numeric" });
const commit = git("rev-parse", "--short", "HEAD");
const branch = git("rev-parse", "--abbrev-ref", "HEAD");
const rustc = (() => {
  try {
    return execFileSync("rustc", ["--version"], { encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
})();

const rows = timings?.rows ?? [];
const models = [...new Set(rows.map((r) => r.model))];
const cmdName = { check: "check", render: "render -o x.png", export_step: "export -o x.step" };
const cell = (s) => (s ? `<b>${de(s.median)}</b> <span class="small">${de(s.min)}–${de(s.max)}</span>` : "–");
const speedup = (r) => (r?.cold && r?.warm ? r.cold.median / r.warm.median : null);
const maxBy = (k) => rows.reduce((m, r) => Math.max(m, r[k]?.median ?? 0), 0);
const minSpeed = rows.length ? Math.min(...rows.map(speedup)) : null;
const maxSpeed = rows.length ? Math.max(...rows.map(speedup)) : null;
const renderRows = rows.filter((r) => r.command === "render");
const coldRender = renderRows.map((r) => r.cold.median);
const warmRender = renderRows.map((r) => r.warm.median);

const startDiffs = rows.map((r) => r.start.median - r.cold.median);
const startExtra = startDiffs.length ? [...startDiffs].sort((a, b) => a - b)[Math.floor(startDiffs.length / 2)] : null;
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");
const timingTable = `<table><tr><th>Modell</th><th>Befehl</th><th>kalt ohne Daemon</th><th>1. Aufruf mit Daemon (inkl. Start)</th><th>2. Aufruf</th><th>2. Aufruf nach kleiner Änderung</th><th>kalt ÷ 2.</th></tr>
${rows
  .map(
    (r) =>
      `<tr><td>${esc(r.model)}</td><td><code>${esc(cmdName[r.command] ?? r.command)}</code></td><td>${cell(r.cold)}</td><td>${cell(r.start)}</td><td>${cell(r.warm)}</td><td>${cell(r.changed)}</td><td>${de(speedup(r), 1)}×</td></tr>`,
  )
  .join("\n")}</table>`;

const changes = {
  "spec/variabler_radius.scad": "Tasche <code>cube([30, 14, 10])</code> → <code>[30, 15, 10]</code> (ein Körper von dreien neu, mit variabler Verrundung)",
  "projekt-gehaeuse/gehaeuse.scad": "Deckel um 1 mm verschoben (<code>tiefe + abstand + 1</code>; nur die Wurzel neu)",
  "spec/lochplatte.scad": "Stift <code>d = 3.5</code> → <code>3.6</code> (Zylinder und Wurzel neu)",
};

const sizeTable = (() => {
  const top = [...sizeRows].sort((a, b) => b.freed - a.freed).slice(0, 8);
  const r = (x) => `<tr><td>${esc(x.file.replace(/^.*examples\//, ""))}</td><td>${x.breps}</td><td>${x.faces}</td><td>${x.edges}</td><td>${mib(x.freed, 2)}</td><td>${mib(x.estimate_topology, 2)} (${de(x.estimate_topology / x.freed, 2)})</td><td>${mib(x.estimate_probe, 2)} (${de(x.estimate_probe / x.freed, 2)})</td></tr>`;
  return `<table><tr><th>Modell</th><th>Breps</th><th>Flächen</th><th>Kanten</th><th>gehalten (MiB)</th><th>Schätzung Topologie (÷ gehalten)</th><th>Daemon: Heap-Messung (÷ gehalten)</th></tr>
${top.map(r).join("\n")}
${sizeSum ? `<tr><th>alle ${sizeSum.models} Modelle</th><th>${sizeSum.breps}</th><th>${sizeSum.faces}</th><th>${sizeSum.edges}</th><th>${mib(sizeSum.freed, 1)}</th><th>${mib(sizeSum.estimate_topology, 1)} (${de(sizeSum.estimate_topology / sizeSum.freed, 2)})</th><th>${mib(sizeSum.estimate_probe, 1)} (${de(sizeSum.estimate_probe / sizeSum.freed, 2)})</th></tr>` : ""}</table>`;
})();
const ratios = sizeRows.map((x) => x.estimate_probe / x.freed).sort((a, b) => a - b);
const topoRatios = sizeRows.map((x) => x.estimate_topology / x.freed).sort((a, b) => a - b);
const perFace = sizeRows.map((x) => x.freed / x.faces).sort((a, b) => a - b);
const med = (a) => (a.length ? a[Math.floor(a.length / 2)] : null);

const testDesc = {
  autostart_then_cache_hits_and_faster: "erster Aufruf startet den Daemon (`daemon.used`), zweiter Aufruf: alle Knoten aus dem Cache, 0 Kernel-Aufrufe, gleiche Ausgabe, Trefferzähler steigt, messbar schneller",
  output_identical_with_and_without_daemon: "Daemon erst mit drei anderen Modellen gefüllt; dann je Befehl lokal, 1. und 2. Aufruf mit Daemon: check (ok, Syntaxfehler, Kernelfehler), eval, eval --module, inspect, outline, measure (Abstand/Volumen, Kollision/Abstand), set --dry-run, export mit unbekanntem Bauteil (Exit 2) – Text und JSON; render (Kontaktbogen, Schnitt, Röntgen) PNG byteweise, STL, 3MF, STEP (ohne Zeitstempel), --all-parts in einen Ordner",
  relative_paths_and_stdin: "relativer Pfad mit anderem Arbeitsverzeichnis (gleiche Ausgabe), Quelltext über stdin (`-`)",
  parallel_starts_give_one_daemon: "sechs gleichzeitige Aufrufe ohne Daemon: alle nutzen den Daemon, genau ein Prozess `cad daemon serve` bleibt",
  idle_end_removes_socket_and_lock: "Leerlauf 1 s: Daemon endet, Socket und Sperrdatei entfernt, Log „ohne Anfrage“",
  version_change_replaces_daemon: "Build-ID a → b: Client beendet den alten Daemon und startet einen neuen; `status` eines dritten Builds meldet die Abweichung",
  switched_off_by_flag_env_and_config: "`--no-daemon` (vor und nach dem Befehl), `CAD_NO_DAEMON=1`, `[daemon] enabled = false`: kein Daemon, Grund in `daemon.reason`, Text still, `--verbose` nennt den Grund",
  fallback_when_socket_is_broken: "Socket-Ordner ist eine Datei; fremder Prozess am Socket, der jede Verbindung schließt; veralteter Socket ohne Prozess (wird ersetzt)",
  cache_limit_and_project_limit: "Grenze 1 MiB und 2 Projekte bei 7 Modellen: Verdrängungen, Größe unter der Grenze (oder nur der neueste Eintrag), 5 Projekte verdrängt; 512 MiB: nichts verdrängt",
  changed_files_are_never_stale: "Hauptdatei, eingebundene Datei, Rückkehr zu altem Stand, `set` über den Client, Syntaxfehler und Korrektur – jedes Ergebnis passt zum aktuellen Inhalt",
  status_and_stop: "`daemon status` (Text, JSON-Felder), `daemon stop` (PID, Socket entfernt), zweites `stop`: „kein Daemon“",
  watchdog_ends_daemon_and_command_runs_locally: "Watchdog 1 s, künstlich hängender Befehl (8 s): Daemon endet, Befehl läuft ohne Daemon mit gleicher Ausgabe, Meldung auf stderr, nächster Aufruf startet einen neuen Daemon",
};

const demo = timings?.demo ?? [];
const demoText = demo
  .map((d) => `$ ${d.cmd}\n${d.stderr ? d.stderr.replace(/\n$/, "") + "\n" : ""}${d.stdout.replace(/\n$/, "")}`)
  .join("\n");
const logText = (timings?.log ?? "").split("\n").filter(Boolean).slice(-6).join("\n");

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 10 – CLI-Daemon</title>
<style>
  @page { size: A4; margin: 15mm 13mm 16mm 13mm; }
  :root { --text: #1f2328; --muted: #5d646e; --border: #d5d9df; --ok: #1a7f37; --bad: #c9302c; --head: #f2f4f7; }
  * { box-sizing: border-box; }
  body { font: 9.6pt/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: var(--text); margin: 0; }
  main { max-width: 184mm; margin: 0 auto; }
  h1 { font-size: 18pt; margin: 0 0 2mm; }
  h2 { font-size: 13pt; margin: 7mm 0 3mm; border-bottom: 1px solid var(--border); padding-bottom: 1mm; break-after: avoid; }
  h3 { font-size: 10.5pt; margin: 4mm 0 1.5mm; break-after: avoid; }
  p { margin: 1.5mm 0; }
  .meta { color: var(--muted); margin-bottom: 4mm; }
  .summary { display: flex; flex-wrap: wrap; gap: 2.5mm; margin: 3mm 0; }
  .summary div { flex: 1 1 28mm; border: 1px solid var(--border); border-radius: 2mm; padding: 2mm; font-size: 8.4pt; }
  .summary b { display: block; font-size: 12pt; }
  table { width: 100%; border-collapse: collapse; margin: 2mm 0; font-size: 8.2pt; }
  th, td { border: 1px solid var(--border); padding: 1.1mm 1.8mm; text-align: left; vertical-align: top; }
  th { background: var(--head); font-weight: 600; }
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 7.6pt; }
  pre { margin: 0 0 1.5mm; padding: 1.4mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  .small { font-size: 7.6pt; color: var(--muted); font-weight: 400; }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.5mm 0; }
  tr, td { break-inside: avoid; }
  .ok { color: var(--ok); font-weight: 600; } .bad { color: var(--bad); font-weight: 600; }
  .keep { break-inside: avoid; }
</style>
</head>
<body>
<main>
<h1>Testbericht 10 – CLI-Daemon</h1>
<div class="meta">cad-cli (daemon, client, host, config), cad-ipc, cad-engine, cad-session, kernel-smlib · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} Kerne (${esc(os.cpus()[0]?.model ?? "")}) · Messung mit <code>target/release/cad</code></div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>${cargoSum ? `${cargoSum.ignored} ignoriert` : ""}</div>
  <div>Daemon-Tests<b class="${daemonRes.length && daemonRes.every((t) => t.res === "ok") ? "ok" : "bad"}">${daemonRes.filter((t) => t.res === "ok").length} / ${daemonRes.length}</b><code>crates/cad-cli/tests/daemon.rs</code></div>
  <div>clippy · fmt<b class="${clippy != null && !/^(error|warning)/m.test(clippy) && fmt != null && fmt.trim() === "" ? "ok" : "bad"}">${clippy == null ? "–" : /^(error|warning)/m.test(clippy) ? "Befund" : "sauber"} · ${fmt == null ? "–" : fmt.trim() === "" ? "sauber" : "Abw."}</b>-D warnings</div>
  <div><code>render</code> kalt → 2. Aufruf<b>${de(Math.min(...coldRender))}–${de(Math.max(...coldRender))} → ${de(Math.min(...warmRender))}–${de(Math.max(...warmRender))} ms</b>Median, drei Modelle</div>
  <div>Beschleunigung 2. Aufruf<b>${de(minSpeed, 1)}–${de(maxSpeed, 1)}×</b>über alle 9 Messreihen</div>
  <div>Leerer Aufruf (cube)<b>${de(timings?.tiny_check_daemon?.median, 1)} / ${de(timings?.tiny_check_local?.median, 1)} ms</b>mit / ohne Daemon</div>
  <div>Speicherschätzung<b>${de(med(ratios), 2)}× (${de(ratios[0], 2)}–${de(ratios.at(-1), 2)})</b>Heap-Messung ÷ gehalten, ${sizeRows.length} Modelle</div>
</div>

<h2>1 Auftrag und Ergebnis</h2>
<p>Bericht 8 maß für einen einzelnen <code>cad render</code> 0,4–1,1 s, davon der Kernel bis 640 ms; <code>cad watch</code> mit warmem Cache 24 ms. Jeder CLI-Aufruf begann mit leerem Cache. Jetzt hält ein Hintergrundprozess <code>cad daemon serve</code> den Engine-Cache über Aufrufe hinweg: der erste rechnende Befehl startet ihn, nach 15 min ohne Anfrage endet er, die Cache-Größe ist begrenzt, und mit <code>--no-daemon</code>, <code>CAD_NO_DAEMON=1</code> oder <code>[daemon] enabled = false</code> ist er aus. Ausgabe, Exit-Codes, Dateien und JSON sind dieselben wie ohne Daemon; ein zweiter Aufruf auf derselben Datei braucht ${de(Math.min(...rows.map((r) => r.warm.median)))}–${de(maxBy("warm"))} ms statt ${de(Math.min(...rows.map((r) => r.cold.median)))}–${de(maxBy("cold"))} ms.</p>

<h2>2 Architektur</h2>
<pre>cad check|eval|inspect|render|export|outline|measure|set …     (Client, jeder Aufruf ein Prozess)
  1 cad-ipc: connect("daemon") → hello {proto, build_id, pid}
      kein Daemon  → cad daemon serve starten (setsid, stdin /dev/null, stdout/stderr → daemon.log),
                      bis 5 s auf hello warten
      andere Build-ID → stop (graceful, nach 3 s SIGKILL), neuen starten
      Fehler (kein Ordner, fremder Socket, Windows) → still lokal rechnen
  2 run {argv, cwd, stdin?} ─────────▶  Verbindungs-Thread ── Kanal ──▶ Arbeits-Thread (256 MiB Stack)
     ◀── accepted, wait (1 s Takt)       (je Verbindung)                 eine Engine&lt;SmlibKernel&gt; für alle Projekte
     ◀── stdout, stderr, exit, Dateien                                   chdir(cwd); Cli::try_parse_from(argv);
  3 Dateien schreiben (atomar bei set),                                  run(&amp;cmd) mit Sitzung auf der Engine
    stdout/stderr ausgeben, Exit-Code                                    (host.rs: Dateien aufzeichnen, stdin)
                                         Überwachungs-Thread: Leerlauf 15 min, Watchdog 60 s, Socket vorhanden?
daemon.lock (flock, PID + Build) · daemon.sock (0600) · daemon.log  in  $CAD_IPC_DIR | $XDG_RUNTIME_DIR/controller-cad | /tmp/controller-cad-&lt;uid&gt;</pre>
<ul>
<li><b>Gleicher Code mit und ohne Daemon.</b> Die Befehle der CLI laufen unverändert; drei Stellen gehen über <code>host.rs</code>: <code>load()</code> holt eine Sitzung über <code>host::new_session</code> (im Daemon auf dessen Engine, <code>Session::with_engine</code>; beim Ende des Befehls gibt <code>host::Pooled</code> die Engine zurück), stdin kommt vom Client, und Dateischreiben (<code>render</code>-PNG, <code>export</code>, <code>--all-parts</code>-Ordner, <code>set</code> atomar) wird im Daemon aufgezeichnet und vom Client ausgeführt. Ausgabe und Exit-Code baut dieselbe Funktion <code>finish()</code> im Daemon wie lokal. Schlägt das Schreiben beim Client fehl, rechnet er lokal neu und gibt so dieselbe Fehlermeldung.</li>
<li><b>Gleiche Flächen-Tags.</b> Die Engine vergibt Tags in der Reihenfolge, in der sie Knoteninhalte zum ersten Mal sieht; in einer Engine, die schon andere Modelle gerechnet hat, sind die Nummern andere als in einem frischen Prozess (sichtbar in <code>eval --json</code>: <code>nodes[].tag</code>, <code>tag_nodes</code>). Die Sitzung nummeriert Tags deshalb selbst in der Reihenfolge ihrer Jobs – genau die Nummern einer frischen Engine – und übersetzt Szene und Meshes (<code>render</code> färbt über diese Zuordnung). Mit eigener Engine ist die Übersetzung die Identität und kostet nichts (Browser, Desktop).</li>
<li><b>Arbeitsverzeichnis:</b> der Daemon wechselt vor jedem Befehl in das Verzeichnis des Clients (danach nach <code>/</code>); relative Pfade, Meldungen und <code>include</code> verhalten sich wie lokal. Ein Arbeits-Thread, die Befehle laufen nacheinander (SMLib hat globalen Zustand).</li>
<li><b>Genau ein Daemon:</b> <code>flock</code> auf <code>daemon.lock</code> (neu in <code>cad-ipc</code>: <code>LockFile</code>, Datei beim Ende entfernt, Inode-Prüfung gegen das Entfernen durch den Vorgänger). Wer die Sperre nicht bekommt, wartet bis 10 s und endet, sobald ein Daemon antwortet; mehrere gleichzeitige Clients starten so höchstens Prozesse, die sofort wieder gehen.</li>
<li><b>Version:</b> <code>hello</code> liefert Protokoll und Build-ID (Version + Größe und Änderungszeit des Programms); bei Abweichung beendet der Client den alten Daemon und startet einen neuen.</li>
<li><b>Watchdog:</b> läuft ein Befehl länger als 60 s, meldet der Daemon den wartenden Clients „watchdog“ und endet (der hängende Kernel-Aufruf lässt sich nicht abbrechen, wie beim Engine-Prozess der Desktop-App). Der Client wiederholt den Befehl ohne Daemon und schreibt das auf stderr. Ohne Lebenszeichen 10 s (<code>wait</code> kommt jede Sekunde) beendet der Client den Daemon (SIGKILL) und rechnet selbst.</li>
<li><b>Sicherheit:</b> nur der Unix-Socket aus <code>cad-ipc</code> (Ordner 0700 des Benutzers, Socket 0600, Peer-UID), kein Netzwerk; Anfragen nur <code>hello</code>, <code>run</code>, <code>status</code>, <code>stop</code>; <code>run</code> nur für die acht Befehle (mit dem Parser der CLI geprüft). Der Daemon liest Dateien als derselbe Benutzer und schreibt keine.</li>
</ul>
<h3>Beispiel</h3>
<pre>${esc(demoText)}</pre>
<p class="small">Log (<code>daemon.log</code> im Socket-Ordner):</p>
<pre>${esc(logText)}</pre>

<h2>3 Grenzen und Konfiguration</h2>
<table><tr><th>Einstellung</th><th>Standard</th><th>Datei <code>[daemon]</code></th><th>Umgebung</th><th><code>cad daemon serve</code></th></tr>
<tr><td>Daemon an/aus</td><td>an</td><td><code>enabled = false</code> (oder <code>daemon = false</code> oben)</td><td><code>CAD_NO_DAEMON=1</code></td><td>– (je Aufruf <code>--no-daemon</code>)</td></tr>
<tr><td>Cache (Knotenergebnisse + Meshes)</td><td>512 MiB</td><td><code>cache_mb</code></td><td><code>CAD_DAEMON_CACHE_MB</code></td><td><code>--cache-mb</code></td></tr>
<tr><td>Einträge</td><td>16384</td><td><code>cache_entries</code></td><td><code>CAD_DAEMON_CACHE_ENTRIES</code></td><td>–</td></tr>
<tr><td>Projekte im Cache</td><td>8</td><td><code>max_projects</code></td><td><code>CAD_DAEMON_MAX_PROJECTS</code></td><td><code>--max-projects</code></td></tr>
<tr><td>Leerlauf bis Ende</td><td>15 min</td><td><code>idle_minutes</code>, <code>idle_secs</code></td><td><code>CAD_DAEMON_IDLE_MINUTES</code>, <code>_IDLE_SECS</code></td><td><code>--idle-minutes</code>, <code>--idle-secs</code></td></tr>
<tr><td>Watchdog</td><td>60 s</td><td><code>watchdog_secs</code></td><td><code>CAD_DAEMON_WATCHDOG_SECS</code></td><td><code>--watchdog-secs</code></td></tr>
<tr><td>RSS-Grenze</td><td>Start + 1,5 × Cache</td><td><code>rss_factor</code></td><td><code>CAD_DAEMON_RSS_FACTOR</code></td><td>–</td></tr>
</table>
<p>Datei: <code>$CAD_CONFIG</code>, sonst <code>$XDG_CONFIG_HOME/controller-cad/config.toml</code>, sonst <code>~/.config/controller-cad/config.toml</code>; Vorrang Argumente &gt; Umgebung &gt; Datei &gt; Standard; ungültige Werte werden ignoriert (Log, mit <code>--verbose</code> auf stderr).</p>
<ul>
<li><b>Cache:</b> ein LRU der Engine für Knotenergebnisse und Meshes zusammen (vorher: 4096 Knoten ohne Größe, Meshes 256 MiB getrennt), mit Bytegrenze und Eintragsgrenze; der neueste Eintrag bleibt auch, wenn er allein darüber liegt. Eine Engine für alle Projekte, die Schlüssel sind inhaltsbasiert.</li>
<li><b>Projekte:</b> je Hauptdatei (absoluter Pfad; stdin je Arbeitsverzeichnis) merkt sich der Daemon die Schlüssel, die ihre Befehle berührt haben. Beim 9. Projekt verliert das am längsten unbenutzte seine Einträge, soweit kein anderes gehaltenes Projekt sie nutzt.</li>
<li><b>RSS:</b> nach jeder Anfrage aus <code>/proc/self/statm</code>; über Start-RSS + 1,5 × Cache-Grenze wird der Cache geleert und freier Speicher mit <code>malloc_trim</code> zurückgegeben. Die Start-RSS (rund 35–40 MiB: Programm, SMLib, Tauri-Bibliotheken) zählt nicht gegen die Grenze, sonst griffe eine kleine Grenze immer.</li>
<li><b>Lebensdauer:</b> Ende nach 15 min ohne <code>run</code> (<code>status</code> zählt nicht), nach <code>cad daemon stop</code> (wartet auf laufende Befehle), wenn der Socket entfernt oder ersetzt wird (z. B. Testordner gelöscht), nach dem Watchdog; Socket und Sperrdatei werden entfernt.</li>
</ul>

<h3>Speicherschätzung</h3>
<p>Kernel-Ergebnisse sind Handles in SMLib; ihre Größe kennt nur der C++-Heap. Gemessen mit <code>crates/cad-cli/examples/cache_size.rs</code>: jedes der ${sizeRows.length} Beispielmodelle mit frischer Engine auswerten, dann die Engine fallen lassen und die freigegebenen Heap-Bytes zählen (glibc <code>mallinfo2</code>). Die Breps halten ${de(perFace[0] / 1024)}–${de(perFace.at(-1) / 1024)} KiB je Fläche (Median ${de(med(perFace) / 1024)} KiB), Freiformflächen und variable Verrundungen am meisten. Die erste Schätzung aus der Topologie (4 KiB + 3 KiB/Fläche + 1 KiB/Kante) lag um den Faktor 6 zu niedrig. Jetzt: <code>Kernel::shape_bytes</code> für SMLib 8 KiB + 32 KiB je Fläche + 1 KiB je Kante (Summe über alle Beispiele ${sizeSum ? de(sizeSum.estimate_topology / sizeSum.freed, 2) : "–"}×, je Modell ${de(topoRatios[0], 2)}–${de(topoRatios.at(-1), 2)}×) und im Daemon zusätzlich eine Messung je Knoten: Heap nach minus vor der Auswertung des Knotens (<code>Engine::set_heap_probe</code>), was die neuen Breps des Knotens samt SMLib-Zwischendaten erfasst. Damit liegt die Schätzung je Modell bei ${de(ratios[0], 2)}–${de(ratios.at(-1), 2)}× des tatsächlich Gehaltenen (Median ${de(med(ratios), 2)}), nie darunter.</p>
${sizeTable}
<p class="small">Die acht größten Modelle; „gehalten“ = beim Verwerfen der Engine freigegeben. Meshes zählen mit 4 Byte je Array-Element (genau).</p>

<h2>4 Messung</h2>
<p>Je Zeile ${timings?.meta?.repeat ?? "–"} Wiederholungen, jede mit gestopptem Daemon und frischer Kopie der Datei; Median (fett), daneben Minimum–Maximum, Wanduhrzeit des <code>cad</code>-Prozesses in ms (${esc(timings?.meta?.date ?? "")}, ${timings?.meta?.cpus ?? "–"} Kerne). „Kalt ohne Daemon“ = Verhalten bis Bericht 9. Kleine Änderungen:</p>
<ul>${models.map((m) => `<li><code>${esc(m)}</code>: ${changes[m] ?? ""}</li>`).join("")}</ul>
${timingTable}
<ul>
<li>Der 1. Aufruf mit Daemon kostet etwa so viel wie kalt ohne Daemon: im Median ${de(startExtra)} ms mehr (Prozessstart des Daemons, Warten auf <code>hello</code>, eine zusätzliche Übertragung), je Messreihe ${de(Math.min(...startDiffs))} bis +${de(Math.max(...startDiffs))} ms – innerhalb der Streuung der Kaltzeit.</li>
<li>Der 2. Aufruf rechnet nichts im Kernel: <code>check</code> braucht nur Parser, Auswertung und den Weg über den Socket; <code>render</code> zusätzlich Rasterisierung und PNG (das Mesh kommt aus dem Cache), <code>export -o x.step</code> den STEP-Writer.</li>
<li>Nach einer kleinen Änderung rechnet der Kernel nur die betroffenen Knoten (Spalte „nach kleiner Änderung“). Bei <code>variabler_radius</code> dominiert die eine neue variable Verrundung; beim STEP-Export kommt die B-Rep der geänderten Wurzel dazu, die neu aus SMLib gelesen und geschrieben wird.</li>
<li>Ein leerer Aufruf (<code>check</code> von <code>cube(1)</code>) kostet mit Daemon ${de(timings?.tiny_check_daemon?.median, 1)} ms, ohne ${de(timings?.tiny_check_local?.median, 1)} ms: Prozessstart des Clients und Socket statt SMLib-Initialisierung im Prozess.</li>
</ul>

<h2>5 Tests</h2>
<ul>
<li><code>cargo test --workspace</code>: ${cargoSum ? `${cargoSum.ok} bestanden, ${cargoSum.failed} fehlgeschlagen, ${cargoSum.ignored} ignoriert` : "–"}; <code>cargo clippy --workspace --all-targets -- -D warnings</code>: ${clippy == null ? "–" : /^(error|warning)/m.test(clippy) ? "Befund" : "ohne Befund"}; <code>cargo fmt --all --check</code>: ${fmt == null ? "–" : fmt.trim() === "" ? "ohne Befund" : "Abweichungen"}.</li>
<li>Jeder Test hat einen eigenen Socket-Ordner (<code>CAD_IPC_DIR</code> in einem temporären Verzeichnis) und beendet seinen Daemon (Guard mit <code>cad daemon stop</code>); löscht der Test den Ordner, endet der Daemon spätestens 0,5 s später von selbst (Socket fehlt). Die übrigen CLI-Tests (<code>cli.rs</code>, <code>agent.rs</code>, <code>projects.rs</code>) laufen mit <code>CAD_NO_DAEMON=1</code>.</li>
<li>CI: <code>scripts/ci/rust.sh test</code> prüft nach <code>cargo test</code>, dass kein <code>cad daemon serve</code> übrig ist (in CI, sonst Fehler); <code>scripts/ci/compat.sh compare|step</code> laufen jetzt über den Daemon (eigener Socket-Ordner, danach <code>status</code> und <code>stop</code>), so prüfen der OpenSCAD-Vergleich und die STEP-Prüfung mit OpenCASCADE auch die Ausgabe des Daemons; die Tauri-E2E-Tests rufen <code>cad</code> mit <code>CAD_NO_DAEMON=1</code> auf.</li>
</ul>
<table><tr><th>Test (<code>crates/cad-cli/tests/daemon.rs</code>)</th><th>Prüft</th><th>Ergebnis</th></tr>
${daemonRes.map((t) => `<tr><td><code>${esc(t.name)}</code></td><td>${md(testDesc[t.name] ?? "")}</td><td class="${t.res === "ok" ? "ok" : "bad"}">${esc(t.res)}</td></tr>`).join("\n")}
${[...ipcRes, ...unitRes].map((t) => `<tr><td><code>${esc(t.name)}</code></td><td>${t.name.includes("lock") ? "<code>cad-ipc</code>: Sperrdatei exklusiv (auch im selben Prozess), 0600, beim Ende entfernt" : t.name.startsWith("config") ? "Konfigurationsdatei: Werte, unbekannte Schlüssel, <code>daemon = false</code>, <code>enabled = false</code>, kaputtes TOML" : "LRU: Eintragsgrenze, <code>retain</code>, Verdrängungszähler"}</td><td class="${t.res === "ok" ? "ok" : "bad"}">${esc(t.res)}</td></tr>`).join("\n")}
</table>
${daemonTimes ? `<p class="small">Im Test <code>autostart_then_cache_hits_and_faster</code> (Debug-Build, <code>eval --json</code> von <code>variabler_radius</code>): 1. Aufruf ${de(daemonTimes.first)} ms, 2. Aufruf ${de(daemonTimes.second)} ms.</p>` : ""}

<h2>6 Bekannte Grenzen</h2>
<ul>
<li>Nur Unix (Linux, macOS; Peer-UID-Prüfung nur unter Linux, sonst schützt der 0700-Ordner). Unter Windows rechnet jeder Befehl selbst, still.</li>
<li>Ein Arbeits-Thread: gleichzeitige CLI-Aufrufe warten aufeinander (ohne Daemon liefen sie parallel in eigenen Prozessen); <code>wait</code>-Ereignisse halten die Clients am Leben.</li>
<li>Der Watchdog trifft auch einen legitim langen Befehl (&gt; 60 s): der Daemon endet, der Befehl läuft ein zweites Mal ohne Daemon. Ein Absturz im Kernel (Signal) beendet den Daemon ebenso; die lokale Wiederholung kann genauso abstürzen. Getestet ist der Watchdog mit einer künstlichen Verzögerung (<code>CAD_DAEMON_DEBUG_HANG_MS</code>), nicht mit einem echten hängenden SMLib-Aufruf.</li>
<li>Speichermessung je Knoten nur mit glibc; Allokationen anderer Threads (Verbindungen) zählen mit. Ohne glibc gilt die Topologie-Schätzung (je Modell Faktor ${de(topoRatios[0], 1)}–${de(topoRatios.at(-1), 1)}). Die RSS wird nach jeder Anfrage geprüft, nicht während einer Auswertung.</li>
<li>Abweichungen zur Ausgabe ohne Daemon: das zusätzliche JSON-Feld <code>daemon</code>; Zähler und Zeiten (<code>stats.cache_hits</code>, <code>kernel_calls</code>, <code>cached</code>, <code>*_ms</code>, <code>timings</code>, die Zeile „Statistik“ von <code>eval</code>) zeigen, was tatsächlich passiert ist. Bei einer Panik sind Meldung und Exit-Code gleich, die Panikmeldung des Laufzeitsystems steht aber im Log statt auf stderr.</li>
<li>Build-ID aus Größe und Änderungszeit des Programms: wechseln sich zwei <code>cad</code>-Binaries ab (Debug und Release), ersetzen sie einander den Daemon (Cache weg). Ein alter Daemon, der noch rechnet, wird nach 3 s hart beendet; dessen Client rechnet dann selbst.</li>
<li>Die Tag-Tabelle der Engine wächst weiter (ein Eintrag je gesehenem Knoteninhalt); nach 2²⁰ Tags ersetzt der Daemon die Engine samt Cache.</li>
<li><code>cad watch</code> nutzt den Daemon nicht (eigener Cache im Prozess); <code>cad set</code> prüft die Änderung mit einer zweiten, kernel-losen Sitzung ohne Cache (wie bisher).</li>
</ul>

<h2>7 Reproduktion</h2>
<pre>export SMLIB_PREBUILT=$PWD/smlib/build-native
cargo build --release -p cad-cli --bin cad --example cache_size
python3 tools/report/report10-measure.py --cad target/release/cad --repeat ${timings?.meta?.repeat ?? 5}
target/release/examples/cache_size examples/spec/*.scad examples/projekt-gehaeuse/gehaeuse.scad \\
    examples/openscad-compat/*.scad &gt; reports/tmp/report10/cache_size.jsonl
cargo test --workspace --no-fail-fast &gt; reports/tmp/report10/cargo.txt 2&gt;&amp;1
cargo test -p cad-cli --test daemon -- --nocapture --test-threads 1 &gt; reports/tmp/report10/daemon-tests.txt 2&gt;&amp;1
cargo clippy --workspace --all-targets -- -D warnings &gt; reports/tmp/report10/clippy.txt 2&gt;&amp;1
cargo fmt --all --check &gt; reports/tmp/report10/fmt.txt
node tools/report/report10.mjs
node tools/report/print-pdf.mjs docs/reports/10-cli-daemon/report.html docs/reports/10-cli-daemon/report.pdf "Testbericht 10: CLI-Daemon"</pre>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${relative(root, join(out, "report.html"))}: ${rows.length} Messreihen, Rust ${cargoSum?.ok ?? "–"}, Daemon-Tests ${daemonRes.length}`);
