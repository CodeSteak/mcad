#!/usr/bin/env node
// Test report 10 ("CLI daemon"): collects the results of the test runs and writes
//
//   docs/reports/10-cli-daemon/report.html
//
// Inputs (gitignored, reports/tmp/report10/):
//   timings.json      python3 tools/report/report10-measure.py --cad target/release/mcad
//   cache_size.jsonl  target/release/examples/cache_size tests/models/spec/*.scad \
//                       tests/models/projekt-gehaeuse/gehaeuse.scad tests/models/openscad-compat/*.scad
//   cargo.txt         cargo test --workspace --no-fail-fast
//   daemon-tests.txt  cargo test -p cad-cli --test daemon -- --nocapture --test-threads 1
//   clippy.txt        cargo clippy --workspace --all-targets -- -D warnings
//   fmt.txt           cargo fmt --all --check (empty = ok)
//
//   node tools/report/report10.mjs
//   node tools/report/print-pdf.mjs docs/reports/10-cli-daemon/report.html docs/reports/10-cli-daemon/report.pdf "Test report 10: CLI daemon"

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
const num = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d }));
const mib = (b, d = 1) => num(b / 1048576, d);

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

const date = new Date().toISOString().slice(0, 10);
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
const cell = (s) => (s ? `<b>${num(s.median)}</b> <span class="small">${num(s.min)}–${num(s.max)}</span>` : "–");
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
const timingTable = `<table><tr><th>Model</th><th>Command</th><th>cold without daemon</th><th>1st call with daemon (incl. start)</th><th>2nd call</th><th>2nd call after small change</th><th>cold ÷ 2nd</th></tr>
${rows
  .map(
    (r) =>
      `<tr><td>${esc(r.model)}</td><td><code>${esc(cmdName[r.command] ?? r.command)}</code></td><td>${cell(r.cold)}</td><td>${cell(r.start)}</td><td>${cell(r.warm)}</td><td>${cell(r.changed)}</td><td>${num(speedup(r), 1)}×</td></tr>`,
  )
  .join("\n")}</table>`;

const changes = {
  "spec/variabler_radius.scad": "pocket <code>cube([30, 14, 10])</code> → <code>[30, 15, 10]</code> (one body of three rebuilt, with variable fillet)",
  "projekt-gehaeuse/gehaeuse.scad": "lid moved by 1 mm (<code>tiefe + abstand + 1</code>; only the root rebuilt)",
  "spec/lochplatte.scad": "pin <code>d = 3.5</code> → <code>3.6</code> (cylinder and root rebuilt)",
};

const sizeTable = (() => {
  const top = [...sizeRows].sort((a, b) => b.freed - a.freed).slice(0, 8);
  const r = (x) => `<tr><td>${esc(x.file.replace(/^.*examples\//, ""))}</td><td>${x.breps}</td><td>${x.faces}</td><td>${x.edges}</td><td>${mib(x.freed, 2)}</td><td>${mib(x.estimate_topology, 2)} (${num(x.estimate_topology / x.freed, 2)})</td><td>${mib(x.estimate_probe, 2)} (${num(x.estimate_probe / x.freed, 2)})</td></tr>`;
  return `<table><tr><th>Model</th><th>Breps</th><th>Faces</th><th>Edges</th><th>held (MiB)</th><th>topology estimate (÷ held)</th><th>daemon: heap measurement (÷ held)</th></tr>
${top.map(r).join("\n")}
${sizeSum ? `<tr><th>all ${sizeSum.models} models</th><th>${sizeSum.breps}</th><th>${sizeSum.faces}</th><th>${sizeSum.edges}</th><th>${mib(sizeSum.freed, 1)}</th><th>${mib(sizeSum.estimate_topology, 1)} (${num(sizeSum.estimate_topology / sizeSum.freed, 2)})</th><th>${mib(sizeSum.estimate_probe, 1)} (${num(sizeSum.estimate_probe / sizeSum.freed, 2)})</th></tr>` : ""}</table>`;
})();
const ratios = sizeRows.map((x) => x.estimate_probe / x.freed).sort((a, b) => a - b);
const topoRatios = sizeRows.map((x) => x.estimate_topology / x.freed).sort((a, b) => a - b);
const perFace = sizeRows.map((x) => x.freed / x.faces).sort((a, b) => a - b);
const med = (a) => (a.length ? a[Math.floor(a.length / 2)] : null);

const testDesc = {
  autostart_then_cache_hits_and_faster: "first call starts the daemon (`daemon.used`), second call: all nodes from the cache, 0 kernel calls, same output, hit counter increases, measurably faster",
  output_identical_with_and_without_daemon: "daemon first filled with three other models; then per command locally, 1st and 2nd call with daemon: check (ok, syntax error, kernel error), eval, eval --module, inspect, outline, measure (distance/volume, collision/clearance), set --dry-run, export with unknown part (exit 2) – text and JSON; render (contact sheet, section, X-ray) PNG byte by byte, STL, 3MF, STEP (without timestamp), --all-parts into a folder",
  relative_paths_and_stdin: "relative path with another working directory (same output), source via stdin (`-`)",
  parallel_starts_give_one_daemon: "six simultaneous calls without daemon: all use the daemon, exactly one `cad daemon serve` process remains",
  idle_end_removes_socket_and_lock: "idle 1 s: daemon exits, socket and lock file removed, log \"without request\"",
  version_change_replaces_daemon: "build ID a → b: client stops the old daemon and starts a new one; `status` of a third build reports the mismatch",
  switched_off_by_flag_env_and_config: "`--no-daemon` (before and after the command), `CAD_NO_DAEMON=1`, `[daemon] enabled = false`: no daemon, reason in `daemon.reason`, text silent, `--verbose` names the reason",
  fallback_when_socket_is_broken: "socket folder is a file; foreign process on the socket that closes every connection; stale socket without process (gets replaced)",
  cache_limit_and_project_limit: "limit 1 MiB and 2 projects with 7 models: evictions, size below the limit (or only the newest entry), 5 projects evicted; 512 MiB: nothing evicted",
  changed_files_are_never_stale: "main file, included file, return to an old state, `set` via the client, syntax error and fix – every result matches the current content",
  status_and_stop: "`daemon status` (text, JSON fields), `daemon stop` (PID, socket removed), second `stop`: \"no daemon\"",
  watchdog_ends_daemon_and_command_runs_locally: "watchdog 1 s, artificially hanging command (8 s): daemon exits, command runs without daemon with the same output, message on stderr, next call starts a new daemon",
};

const demo = timings?.demo ?? [];
const demoText = demo
  .map((d) => `$ ${d.cmd}\n${d.stderr ? d.stderr.replace(/\n$/, "") + "\n" : ""}${d.stdout.replace(/\n$/, "")}`)
  .join("\n");
const logText = (timings?.log ?? "").split("\n").filter(Boolean).slice(-6).join("\n");

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 10 – CLI daemon</title>
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
<h1>Test report 10 – CLI daemon</h1>
<div class="meta">cad-cli (daemon, client, host, config), cad-ipc, cad-engine, cad-session, kernel-smlib · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} cores (${esc(os.cpus()[0]?.model ?? "")}) · measured with <code>target/release/mcad</code></div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>${cargoSum ? `${cargoSum.ignored} ignored` : ""}</div>
  <div>Daemon-Tests<b class="${daemonRes.length && daemonRes.every((t) => t.res === "ok") ? "ok" : "bad"}">${daemonRes.filter((t) => t.res === "ok").length} / ${daemonRes.length}</b><code>crates/cad-cli/tests/daemon.rs</code></div>
  <div>clippy · fmt<b class="${clippy != null && !/^(error|warning)/m.test(clippy) && fmt != null && fmt.trim() === "" ? "ok" : "bad"}">${clippy == null ? "–" : /^(error|warning)/m.test(clippy) ? "finding" : "clean"} · ${fmt == null ? "–" : fmt.trim() === "" ? "clean" : "diff"}</b>-D warnings</div>
  <div><code>render</code> cold → 2nd call<b>${num(Math.min(...coldRender))}–${num(Math.max(...coldRender))} → ${num(Math.min(...warmRender))}–${num(Math.max(...warmRender))} ms</b>median, three models</div>
  <div>Speed-up 2nd call<b>${num(minSpeed, 1)}–${num(maxSpeed, 1)}×</b>over all 9 series</div>
  <div>Empty call (cube)<b>${num(timings?.tiny_check_daemon?.median, 1)} / ${num(timings?.tiny_check_local?.median, 1)} ms</b>with / without daemon</div>
  <div>Memory estimate<b>${num(med(ratios), 2)}× (${num(ratios[0], 2)}–${num(ratios.at(-1), 2)})</b>heap measurement ÷ held, ${sizeRows.length} models</div>
</div>

<h2>1 Task and result</h2>
<p>Report 8 measured 0.4–1.1 s for a single <code>cad render</code>, of which the kernel took up to 640 ms; <code>cad watch</code> with a warm cache 24 ms. Every CLI call started with an empty cache. Now a background process <code>cad daemon serve</code> keeps the engine cache across calls: the first computing command starts it, it ends after 15 min without a request, the cache size is limited, and it is off with <code>--no-daemon</code>, <code>CAD_NO_DAEMON=1</code> or <code>[daemon] enabled = false</code>. Output, exit codes, files and JSON are the same as without the daemon; a second call on the same file takes ${num(Math.min(...rows.map((r) => r.warm.median)))}–${num(maxBy("warm"))} ms instead of ${num(Math.min(...rows.map((r) => r.cold.median)))}–${num(maxBy("cold"))} ms.</p>

<h2>2 Architecture</h2>
<pre>cad check|eval|inspect|render|export|outline|measure|set …     (client, one process per call)
  1 cad-ipc: connect("daemon") → hello {proto, build_id, pid}
      no daemon     → start cad daemon serve (setsid, stdin /dev/null, stdout/stderr → daemon.log),
                      wait up to 5 s for hello
      other build ID → stop (graceful, SIGKILL after 3 s), start a new one
      error (no folder, foreign socket, Windows) → silently compute locally
  2 run {argv, cwd, stdin?} ─────────▶  connection thread ── channel ──▶ worker thread (256 MiB stack)
     ◀── accepted, wait (1 s tick)       (per connection)                one Engine&lt;SmlibKernel&gt; for all projects
     ◀── stdout, stderr, exit, files                                     chdir(cwd); Cli::try_parse_from(argv);
  3 write files (atomic for set),                                        run(&amp;cmd) with a session on the engine
    print stdout/stderr, exit code                                       (host.rs: record files, stdin)
                                         monitor thread: idle 15 min, watchdog 60 s, socket present?
daemon.lock (flock, PID + build) · daemon.sock (0600) · daemon.log  in  $CAD_IPC_DIR | $XDG_RUNTIME_DIR/controller-cad | /tmp/controller-cad-&lt;uid&gt;</pre>
<ul>
<li><b>Same code with and without the daemon.</b> The CLI commands run unchanged; three places go through <code>host.rs</code>: <code>load()</code> gets a session via <code>host::new_session</code> (in the daemon on its engine, <code>Session::with_engine</code>; when the command ends, <code>host::Pooled</code> returns the engine), stdin comes from the client, and file writes (<code>render</code> PNG, <code>export</code>, <code>--all-parts</code> folder, <code>set</code> atomic) are recorded in the daemon and executed by the client. Output and exit code are built by the same function <code>finish()</code> in the daemon as locally. If writing fails on the client, it recomputes locally and thus gives the same error message.</li>
<li><b>Same face tags.</b> The engine assigns tags in the order in which it first sees node contents; in an engine that has already computed other models, the numbers differ from those in a fresh process (visible in <code>eval --json</code>: <code>nodes[].tag</code>, <code>tag_nodes</code>). The session therefore numbers tags itself in the order of its jobs – exactly the numbers of a fresh engine – and translates scene and meshes (<code>render</code> colors via this mapping). With its own engine the translation is the identity and costs nothing (browser, desktop).</li>
<li><b>Working directory:</b> before each command the daemon changes to the client's directory (afterwards to <code>/</code>); relative paths, messages and <code>include</code> behave as locally. One worker thread, commands run one after another (SMLib has global state).</li>
<li><b>Exactly one daemon:</b> <code>flock</code> on <code>daemon.lock</code> (new in <code>cad-ipc</code>: <code>LockFile</code>, file removed at the end, inode check against removal by the predecessor). A process that does not get the lock waits up to 10 s and exits as soon as a daemon responds; several simultaneous clients therefore at most start processes that exit again immediately.</li>
<li><b>Version:</b> <code>hello</code> returns protocol and build ID (version + size and modification time of the executable); on mismatch the client stops the old daemon and starts a new one.</li>
<li><b>Watchdog:</b> if a command runs longer than 60 s, the daemon reports "watchdog" to the waiting clients and exits (the hanging kernel call cannot be aborted, as with the engine process of the desktop app). The client repeats the command without the daemon and says so on stderr. After 10 s without a sign of life (<code>wait</code> arrives every second) the client kills the daemon (SIGKILL) and computes itself.</li>
<li><b>Security:</b> only the Unix socket from <code>cad-ipc</code> (user's folder 0700, socket 0600, peer UID), no network; requests only <code>hello</code>, <code>run</code>, <code>status</code>, <code>stop</code>; <code>run</code> only for the eight commands (checked with the CLI parser). The daemon reads files as the same user and writes none.</li>
</ul>
<h3>Example</h3>
<pre>${esc(demoText)}</pre>
<p class="small">Log (<code>daemon.log</code> in the socket folder):</p>
<pre>${esc(logText)}</pre>

<h2>3 Limits and configuration</h2>
<table><tr><th>Setting</th><th>Default</th><th>File <code>[daemon]</code></th><th>Environment</th><th><code>cad daemon serve</code></th></tr>
<tr><td>Daemon on/off</td><td>on</td><td><code>enabled = false</code> (or <code>daemon = false</code> at top level)</td><td><code>CAD_NO_DAEMON=1</code></td><td>– (per call <code>--no-daemon</code>)</td></tr>
<tr><td>Cache (node results + meshes)</td><td>512 MiB</td><td><code>cache_mb</code></td><td><code>CAD_DAEMON_CACHE_MB</code></td><td><code>--cache-mb</code></td></tr>
<tr><td>Entries</td><td>16384</td><td><code>cache_entries</code></td><td><code>CAD_DAEMON_CACHE_ENTRIES</code></td><td>–</td></tr>
<tr><td>Projects in the cache</td><td>8</td><td><code>max_projects</code></td><td><code>CAD_DAEMON_MAX_PROJECTS</code></td><td><code>--max-projects</code></td></tr>
<tr><td>Idle time until exit</td><td>15 min</td><td><code>idle_minutes</code>, <code>idle_secs</code></td><td><code>CAD_DAEMON_IDLE_MINUTES</code>, <code>_IDLE_SECS</code></td><td><code>--idle-minutes</code>, <code>--idle-secs</code></td></tr>
<tr><td>Watchdog</td><td>60 s</td><td><code>watchdog_secs</code></td><td><code>CAD_DAEMON_WATCHDOG_SECS</code></td><td><code>--watchdog-secs</code></td></tr>
<tr><td>RSS limit</td><td>start + 1.5 × cache</td><td><code>rss_factor</code></td><td><code>CAD_DAEMON_RSS_FACTOR</code></td><td>–</td></tr>
</table>
<p>File: <code>$CAD_CONFIG</code>, else <code>$XDG_CONFIG_HOME/controller-cad/config.toml</code>, else <code>~/.config/controller-cad/config.toml</code>; precedence arguments &gt; environment &gt; file &gt; default; invalid values are ignored (log, with <code>--verbose</code> on stderr).</p>
<ul>
<li><b>Cache:</b> one LRU of the engine for node results and meshes together (before: 4096 nodes without size, meshes 256 MiB separately), with a byte limit and an entry limit; the newest entry stays even if it alone exceeds the limit. One engine for all projects; the keys are content-based.</li>
<li><b>Projects:</b> per main file (absolute path; stdin per working directory) the daemon remembers the keys its commands touched. With the 9th project, the least recently used one loses its entries unless another held project uses them.</li>
<li><b>RSS:</b> after each request from <code>/proc/self/statm</code>; above start RSS + 1.5 × cache limit the cache is cleared and free memory returned with <code>malloc_trim</code>. The start RSS (about 35–40 MiB: executable, SMLib, Tauri libraries) does not count against the limit, otherwise a small limit would always trigger.</li>
<li><b>Lifetime:</b> exit after 15 min without <code>run</code> (<code>status</code> does not count), after <code>cad daemon stop</code> (waits for running commands), when the socket is removed or replaced (e.g. test folder deleted), after the watchdog; socket and lock file are removed.</li>
</ul>

<h3>Memory estimate</h3>
<p>Kernel results are handles in SMLib; only the C++ heap knows their size. Measured with <code>crates/cad-cli/examples/cache_size.rs</code>: evaluate each of the ${sizeRows.length} example models with a fresh engine, then drop the engine and count the freed heap bytes (glibc <code>mallinfo2</code>). The breps hold ${num(perFace[0] / 1024)}–${num(perFace.at(-1) / 1024)} KiB per face (median ${num(med(perFace) / 1024)} KiB), free-form surfaces and variable fillets the most. The first estimate from the topology (4 KiB + 3 KiB/face + 1 KiB/edge) was too low by a factor of 6. Now: <code>Kernel::shape_bytes</code> for SMLib 8 KiB + 32 KiB per face + 1 KiB per edge (sum over all examples ${sizeSum ? num(sizeSum.estimate_topology / sizeSum.freed, 2) : "–"}×, per model ${num(topoRatios[0], 2)}–${num(topoRatios.at(-1), 2)}×) and in the daemon additionally a measurement per node: heap after minus before the evaluation of the node (<code>Engine::set_heap_probe</code>), which captures the new breps of the node including SMLib intermediate data. The estimate per model is thus ${num(ratios[0], 2)}–${num(ratios.at(-1), 2)}× of what is actually held (median ${num(med(ratios), 2)}), never below.</p>
${sizeTable}
<p class="small">The eight largest models; "held" = freed when the engine is dropped. Meshes count 4 bytes per array element (exact).</p>

<h2>4 Measurement</h2>
<p>${timings?.meta?.repeat ?? "–"} repetitions per row, each with a stopped daemon and a fresh copy of the file; median (bold), next to it minimum–maximum, wall-clock time of the <code>cad</code> process in ms (${esc(timings?.meta?.date ?? "")}, ${timings?.meta?.cpus ?? "–"} cores). "Cold without daemon" = behavior up to report 9. Small changes:</p>
<ul>${models.map((m) => `<li><code>${esc(m)}</code>: ${changes[m] ?? ""}</li>`).join("")}</ul>
${timingTable}
<ul>
<li>The 1st call with the daemon costs about as much as cold without the daemon: median ${num(startExtra)} ms more (process start of the daemon, waiting for <code>hello</code>, one additional transfer), per series ${num(Math.min(...startDiffs))} to +${num(Math.max(...startDiffs))} ms – within the spread of the cold time.</li>
<li>The 2nd call computes nothing in the kernel: <code>check</code> needs only parser, evaluation and the round trip over the socket; <code>render</code> additionally rasterization and PNG (the mesh comes from the cache), <code>export -o x.step</code> the STEP writer.</li>
<li>After a small change the kernel computes only the affected nodes (column "after small change"). For <code>variabler_radius</code> the one new variable fillet dominates; for the STEP export the B-rep of the changed root is added, which is read anew from SMLib and written.</li>
<li>An empty call (<code>check</code> of <code>cube(1)</code>) costs ${num(timings?.tiny_check_daemon?.median, 1)} ms with the daemon, ${num(timings?.tiny_check_local?.median, 1)} ms without: process start of the client and socket instead of SMLib initialization in the process.</li>
</ul>

<h2>5 Tests</h2>
<ul>
<li><code>cargo test --workspace</code>: ${cargoSum ? `${cargoSum.ok} passed, ${cargoSum.failed} failed, ${cargoSum.ignored} ignored` : "–"}; <code>cargo clippy --workspace --all-targets -- -D warnings</code>: ${clippy == null ? "–" : /^(error|warning)/m.test(clippy) ? "finding" : "no findings"}; <code>cargo fmt --all --check</code>: ${fmt == null ? "–" : fmt.trim() === "" ? "no findings" : "differences"}.</li>
<li>Each test has its own socket folder (<code>CAD_IPC_DIR</code> in a temporary directory) and stops its daemon (guard with <code>cad daemon stop</code>); if the test deletes the folder, the daemon exits by itself at most 0.5 s later (socket missing). The other CLI tests (<code>cli.rs</code>, <code>agent.rs</code>, <code>projects.rs</code>) run with <code>CAD_NO_DAEMON=1</code>.</li>
<li>CI: <code>scripts/ci/rust.sh test</code> checks after <code>cargo test</code> that no <code>cad daemon serve</code> is left over (in CI, error otherwise); <code>scripts/ci/compat.sh compare|step</code> now run through the daemon (own socket folder, then <code>status</code> and <code>stop</code>), so the OpenSCAD comparison and the STEP check with OpenCASCADE also check the daemon's output; the Tauri E2E tests call <code>cad</code> with <code>CAD_NO_DAEMON=1</code>.</li>
</ul>
<table><tr><th>Test (<code>crates/cad-cli/tests/daemon.rs</code>)</th><th>Checks</th><th>Result</th></tr>
${daemonRes.map((t) => `<tr><td><code>${esc(t.name)}</code></td><td>${md(testDesc[t.name] ?? "")}</td><td class="${t.res === "ok" ? "ok" : "bad"}">${esc(t.res)}</td></tr>`).join("\n")}
${[...ipcRes, ...unitRes].map((t) => `<tr><td><code>${esc(t.name)}</code></td><td>${t.name.includes("lock") ? "<code>cad-ipc</code>: lock file exclusive (also within the same process), 0600, removed at the end" : t.name.startsWith("config") ? "configuration file: values, unknown keys, <code>daemon = false</code>, <code>enabled = false</code>, broken TOML" : "LRU: entry limit, <code>retain</code>, eviction counter"}</td><td class="${t.res === "ok" ? "ok" : "bad"}">${esc(t.res)}</td></tr>`).join("\n")}
</table>
${daemonTimes ? `<p class="small">In the test <code>autostart_then_cache_hits_and_faster</code> (debug build, <code>eval --json</code> of <code>variabler_radius</code>): 1st call ${num(daemonTimes.first)} ms, 2nd call ${num(daemonTimes.second)} ms.</p>` : ""}

<h2>6 Known limitations</h2>
<ul>
<li>Unix only (Linux, macOS; peer UID check only on Linux, otherwise the 0700 folder protects). On Windows every command computes by itself, silently.</li>
<li>One worker thread: simultaneous CLI calls wait for each other (without the daemon they would run in parallel in separate processes); <code>wait</code> events keep the clients alive.</li>
<li>The watchdog also hits a legitimately long command (&gt; 60 s): the daemon exits, the command runs a second time without the daemon. A crash in the kernel (signal) also ends the daemon; the local repetition can crash the same way. The watchdog is tested with an artificial delay (<code>CAD_DAEMON_DEBUG_HANG_MS</code>), not with a real hanging SMLib call.</li>
<li>Memory measurement per node only with glibc; allocations of other threads (connections) are counted too. Without glibc the topology estimate applies (per model factor ${num(topoRatios[0], 1)}–${num(topoRatios.at(-1), 1)}). The RSS is checked after each request, not during an evaluation.</li>
<li>Differences from the output without the daemon: the additional JSON field <code>daemon</code>; counters and timings (<code>stats.cache_hits</code>, <code>kernel_calls</code>, <code>cached</code>, <code>*_ms</code>, <code>timings</code>, the statistics line of <code>eval</code>) show what actually happened. On a panic, message and exit code are the same, but the runtime's panic message goes to the log instead of stderr.</li>
<li>Build ID from size and modification time of the executable: if two <code>cad</code> binaries alternate (debug and release), they replace each other's daemon (cache lost). An old daemon that is still computing is killed after 3 s; its client then computes itself.</li>
<li>The engine's tag table keeps growing (one entry per node content seen); after 2²⁰ tags the daemon replaces the engine together with its cache.</li>
<li><code>cad watch</code> does not use the daemon (own cache in the process); <code>cad set</code> checks the change with a second, kernel-less session without cache (as before).</li>
</ul>

<h2>7 Reproduction</h2>
<pre>export SMLIB_PREBUILT=$PWD/smlib/build-native
cargo build --release -p cad-cli --bin cad --example cache_size
python3 tools/report/report10-measure.py --cad target/release/mcad --repeat ${timings?.meta?.repeat ?? 5}
target/release/examples/cache_size tests/models/spec/*.scad tests/models/projekt-gehaeuse/gehaeuse.scad \\
    tests/models/openscad-compat/*.scad &gt; reports/tmp/report10/cache_size.jsonl
cargo test --workspace --no-fail-fast &gt; reports/tmp/report10/cargo.txt 2&gt;&amp;1
cargo test -p cad-cli --test daemon -- --nocapture --test-threads 1 &gt; reports/tmp/report10/daemon-tests.txt 2&gt;&amp;1
cargo clippy --workspace --all-targets -- -D warnings &gt; reports/tmp/report10/clippy.txt 2&gt;&amp;1
cargo fmt --all --check &gt; reports/tmp/report10/fmt.txt
node tools/report/report10.mjs
node tools/report/print-pdf.mjs docs/reports/10-cli-daemon/report.html docs/reports/10-cli-daemon/report.pdf "Test report 10: CLI daemon"</pre>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${relative(root, join(out, "report.html"))}: ${rows.length} measurement series, Rust ${cargoSum?.ok ?? "–"}, daemon tests ${daemonRes.length}`);
