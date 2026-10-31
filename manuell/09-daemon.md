# mcad daemon

`check`, `eval`, `inspect`, `render`, `export`, `outline`, `measure`, `set` run in a
background process that keeps the kernel cache. Started by the first such call, exits after
15 min idle. Output is identical; JSON adds `daemon: {used, reason}`. The daemon answers in the
language of each call (`--lang`, `MCAD_LANG`, locale of the client), cached results included. The cache is
content-based – results are never stale. The daemon computes with SMLib only: with
`--kernel solidcore` / `MCAD_KERNEL=solidcore` the command runs locally (`daemon.reason`:
"kernel solidcore (the daemon computes with SMLib only)").

| Command / setting | Effect |
| --- | --- |
| `mcad daemon status [--json]` | PID, build, uptime, cache size/hit rate, RSS, socket, log |
| `mcad daemon stop` | stop after the running command |
| `mcad daemon serve [--cache-mb N] [--idle-minutes M] [--max-projects N] [--watchdog-secs S]` | foreground (debugging) |
| `--no-daemon`, `MCAD_NO_DAEMON=1`, `[daemon] enabled = false` | off |

Config `$XDG_CONFIG_HOME/mcad/config.toml` (or `MCAD_CONFIG`), section `[daemon]`:
`enabled`, `cache_mb` (512), `max_projects` (8), `idle_minutes` (15), `watchdog_secs` (1800),
`rss_factor` (1.5); env overrides `MCAD_DAEMON_CACHE_MB`, `MCAD_DAEMON_MAX_PROJECTS`,
`MCAD_DAEMON_IDLE_MINUTES`, `MCAD_DAEMON_WATCHDOG_SECS`, `MCAD_DAEMON_RSS_FACTOR`.
Socket directory (also `mcad ui`): `MCAD_IPC_DIR`, else `$XDG_RUNTIME_DIR/mcad`, else
`/tmp/mcad-<uid>`.

Deprecated names (before the rename `cad` → `mcad`, 0.3.0), still read:

| Old | New | Rule |
| --- | --- | --- |
| `CAD_NO_DAEMON`, `CAD_CONFIG`, `CAD_IPC_DIR`, `CAD_DAEMON_*`, `CAD_WATCHDOG_MS` | `MCAD_…` | old name used only if the `MCAD_…` variable is unset |
| `~/.config/controller-cad/config.toml` | `~/.config/mcad/config.toml` | old file read only if the new one does not exist |

A command that hangs longer than the watchdog ends the daemon and reruns locally. A `mcad` of
another build replaces the running daemon.
