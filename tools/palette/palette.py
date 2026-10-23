#!/usr/bin/env python3
"""Checks of the colour palette (Plan 16.1, test report 11).

Reads the generated table `web/src/logic/palette.ts` (source: crates/cad-eval/src/palette.rs)
and prints / writes (`--json out.json`):

* per colour: OKLCH, display values (light/dark theme), WCAG contrast against the 3D view
  backgrounds of both themes (`--view-bg` in web/src/styles.css);
* simulated colours for deuteranopia, protanopia and tritanopia (Machado, Oliveira,
  Fernandes 2009, severity 1.0, applied in linear sRGB);
* the closest pairs (ΔE in OKLab) for normal vision and each simulation.

The same formulas are in `palette.rs` (`cargo test -p cad-eval --test palette` enforces the
minimum distances). No dependencies besides Python 3.
"""

import argparse
import itertools
import json
import math
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

MACHADO = {
    "deuteranopia": [[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.011820, 0.042940, 0.968881]],
    "protanopia": [[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]],
    "tritanopia": [[1.255528, -0.076749, -0.178779], [-0.078411, 0.930809, 0.147602], [0.004733, 0.691367, 0.303900]],
}


def to_lin(c):
    c /= 255
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def to_srgb(c):
    c = min(1.0, max(0.0, c))
    return round((12.92 * c if c <= 0.0031308 else 1.055 * c ** (1 / 2.4) - 0.055) * 255)


def rgb(h):
    return [int(h[i:i + 2], 16) for i in (1, 3, 5)]


def hexs(c):
    return "#%02x%02x%02x" % tuple(c)


def oklab(c):
    r, g, b = (to_lin(x) for x in c)
    l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b) ** (1 / 3)
    m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b) ** (1 / 3)
    s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b) ** (1 / 3)
    return (0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s)


def oklch(c):
    L, a, b = oklab(c)
    return L, math.hypot(a, b), math.degrees(math.atan2(b, a)) % 360


def simulate(c, kind):
    m = MACHADO[kind]
    lin = [to_lin(x) for x in c]
    return [to_srgb(sum(m[i][j] * lin[j] for j in range(3))) for i in range(3)]


def lum(c):
    r, g, b = (to_lin(x) for x in c)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(a, b):
    x, y = lum(a), lum(b)
    return (max(x, y) + 0.05) / (min(x, y) + 0.05)


def read_palette():
    t = (ROOT / "web/src/logic/palette.ts").read_text()
    rows = re.findall(r'\{ name: "(\w+)", hex: "(#\w+)", light: "(#\w+)", dark: "(#\w+)" \}', t)
    default = re.search(r'DEFAULT_COLOR = "(#\w+)"', t).group(1)
    return [{"name": n, "hex": h, "light": l, "dark": d} for n, h, l, d in rows], default


def view_bgs():
    css = (ROOT / "web/src/styles.css").read_text()
    out = {}
    for theme in ("light", "dark"):
        block = re.search(r':root\[data-theme="%s"\][^{]*\{([^}]*)\}' % theme, css).group(1)
        out[theme] = re.search(r"--view-bg:\s*(#\w+)", block).group(1)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--json")
    a = ap.parse_args()
    pal, default = read_palette()
    bgs = view_bgs()
    colors = []
    for p in pal:
        c = rgb(p["hex"])
        L, C, h = oklch(c)
        colors.append({
            **p,
            "oklch": [round(L, 4), round(C, 4), round(h, 1)],
            "contrast_light_bg": round(contrast(rgb(p["light"]), rgb(bgs["light"])), 2),
            "contrast_dark_bg": round(contrast(rgb(p["dark"]), rgb(bgs["dark"])), 2),
            "sim": {k: hexs(simulate(c, k)) for k in MACHADO},
        })
    pairs = {}
    for kind in ["normal", *MACHADO]:
        labs = [(p["name"], oklab(rgb(p["hex"]) if kind == "normal" else simulate(rgb(p["hex"]), kind))) for p in pal]
        ds = sorted((math.dist(x[1], y[1]), x[0], y[0]) for x, y in itertools.combinations(labs, 2))
        pairs[kind] = [{"a": x, "b": y, "de": round(d, 4)} for d, x, y in ds[:5]]
    out = {"colors": colors, "default": default, "view_bg": bgs, "closest": pairs}
    for c in colors:
        print(f"{c['name']:8s} {c['hex']} OKLCH {c['oklch']}  hell {c['light']} ({c['contrast_light_bg']}:1)  "
              f"dunkel {c['dark']} ({c['contrast_dark_bg']}:1)  Deut {c['sim']['deuteranopia']}")
    for k, v in pairs.items():
        print(f"{k:12s} kleinste ΔE: " + ", ".join(f"{p['a']}/{p['b']} {p['de']:.3f}" for p in v[:3]))
    if a.json:
        Path(a.json).parent.mkdir(parents=True, exist_ok=True)
        Path(a.json).write_text(json.dumps(out, indent=1))


if __name__ == "__main__":
    main()
