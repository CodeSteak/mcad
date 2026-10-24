# cad daemon

`check`, `eval`, `inspect`, `render`, `export`, `outline`, `measure`, `set` run in a
background process that keeps the kernel cache. Started by the first such call, exits after
15 min idle. Output is identical; JSON adds `daemon: {used, reason}`. The cache is
content-based – results are never stale.

| Command / setting | Effect |
| --- | --- |
| `cad daemon status [--json]` | PID, build, uptime, cache size/hit rate, RSS, socket, log |
| `cad daemon stop` | stop after the running command |
| `cad daemon serve [--cache-mb N] [--idle-minutes M] [--max-projects N] [--watchdog-secs S]` | foreground (debugging) |
| `--no-daemon`, `CAD_NO_DAEMON=1`, `[daemon] enabled = false` | off |

Config `$XDG_CONFIG_HOME/controller-cad/config.toml` (or `CAD_CONFIG`), section `[daemon]`:
`enabled`, `cache_mb` (512), `max_projects` (8), `idle_minutes` (15), `watchdog_secs` (60),
`rss_factor` (1.5); env overrides `CAD_DAEMON_CACHE_MB`, `CAD_DAEMON_MAX_PROJECTS`,
`CAD_DAEMON_IDLE_MINUTES`, `CAD_DAEMON_WATCHDOG_SECS`, `CAD_DAEMON_RSS_FACTOR`.

A command that hangs longer than the watchdog ends the daemon and reruns locally. A `cad` of
another build replaces the running daemon.
