# mcad watch

`mcad watch FILE [--json] [--render x.png RENDER-OPTIONS] [--once-after-change] [--timeout S] [--debounce MS]`

Watches the file and everything it loads (includes, SVG, fonts; also files added later) and
re-evaluates on change in the same session (cache kept). One line per run on stdout, with
`--json` one JSON object per line (`trigger: "start"|"change"`, `changed`, errors, parts,
timings, `render`).

Ends on Ctrl+C, after the first change-triggered run (`--once-after-change`) or after S
seconds without change (`--timeout`). Does not use the daemon.

```sh
mcad watch box.mcad --json --render /tmp/box.png --views all --show grids,anchors &
```
