#!/usr/bin/env python3
"""Data of test report 22 (studio look: `mcad render --style studio`). Runs the release CLI
without daemon on the example models and writes images, image measurements and timings to
reports/tmp/report22/data/:

  pairs/*.png        standard and studio of six examples (iso, 900x675)
  shadow/, ao/       shadows and ambient occlusion on/off, AO strength
  ground/            ground and background variants, dark theme
  light/, soft/      light directions, shadow softness
  sheet/             contact sheet --views all (standard, studio)
  section/           section and perspective with the studio look
  data.json          commands, changed pixel shares, brightness of probe regions, timings

  python3 tools/report/report22-data.py [--mcad target/release/mcad] [--before <mcad without the
      studio look>] [--repeat 5]

Timings: `render_ms` of the JSON output (rendering only, evaluation excluded), median of
`--repeat` runs; `--before` (optional) times the standard style of the build before the change.
"""
import argparse
import json
import os
import statistics
import subprocess
from pathlib import Path

from PIL import Image, ImageChops, ImageStat

ROOT = Path(__file__).resolve().parents[2]
ap = argparse.ArgumentParser()
ap.add_argument("--mcad", default=str(ROOT / "target/release/mcad"))
ap.add_argument("--before", default=os.environ.get("BEFORE"))
ap.add_argument("--repeat", type=int, default=5)
args = ap.parse_args()
OUT = ROOT / "reports/tmp/report22/data"
ENV = dict(os.environ, MCAD_NO_DAEMON="1", MCAD_LANG="en")
EX = ROOT / "examples"
data = {"commands": {}, "diff": {}, "probe": {}, "timings": {}}


def render(name, model, *opts, size="900x675", binary=None):
    """Renders `model` with `opts` into OUT/name.png; returns the JSON output."""
    out = OUT / f"{name}.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    cmd = [binary or args.mcad, "render", str(EX / model), "-o", str(out), "--size", size, "--json", *opts]
    p = subprocess.run(cmd, capture_output=True, text=True, env=ENV)
    if p.returncode != 0:
        raise SystemExit(f"{' '.join(cmd)}\n{p.stdout}\n{p.stderr}")
    data["commands"][name] = "mcad render " + " ".join([f"examples/{model}", "-o", f"{Path(name).name}.png", "--size", size, *opts])
    return json.loads(p.stdout)


def changed(a, b, tol=12):
    """Share of pixels of two images that differ by more than `tol` in a channel."""
    x = Image.open(OUT / f"{a}.png").convert("RGB")
    y = Image.open(OUT / f"{b}.png").convert("RGB")
    d = ImageChops.difference(x, y).convert("L").point(lambda v: 255 if v > tol else 0)
    n = sum(1 for v in d.getdata() if v)
    return n / (x.width * x.height)


def luma(name, box):
    """Mean brightness (0…255) of a box (x0, y0, x1, y1) of an image."""
    im = Image.open(OUT / f"{name}.png").convert("L").crop(box)
    return ImageStat.Stat(im).mean[0]


def median_ms(model, *opts, size="1024x768", binary=None):
    vals = []
    for _ in range(args.repeat):
        j = render("tmp/timing", model, *opts, size=size, binary=binary)
        vals.append(j["timings"]["render_ms"])
    return statistics.median(vals)


MODELS = ["02-bracket.mcad", "03-tray.mcad", "04-hook.mcad", "05-box-lid.mcad", "07-mug.mcad", "08-macropad.mcad"]

# 1. standard and studio side by side
for m in MODELS:
    stem = m.removesuffix(".mcad")
    render(f"pairs/{stem}-standard", m)
    j = render(f"pairs/{stem}-studio", m, "--style", "studio")
    data["diff"][f"pairs/{stem}"] = changed(f"pairs/{stem}-standard", f"pairs/{stem}-studio")
    data.setdefault("studio_views", {})[stem] = j["views"][0]["studio"]

# 2. shadows and ambient occlusion
render("shadow/on", "02-bracket.mcad", "--style", "studio", "--ao", "off")
render("shadow/off", "02-bracket.mcad", "--style", "studio", "--ao", "off", "--shadows", "off")
data["diff"]["shadow"] = changed("shadow/on", "shadow/off")
render("ao/on", "03-tray.mcad", "--style", "studio", "--shadows", "off")
render("ao/off", "03-tray.mcad", "--style", "studio", "--shadows", "off", "--ao", "off")
render("ao/strong", "03-tray.mcad", "--style", "studio", "--shadows", "off", "--ao-strength", "2", "--ao-radius", "6")
data["diff"]["ao"] = changed("ao/on", "ao/off")
data["diff"]["ao_strong"] = changed("ao/strong", "ao/off")
render("ao/box-on", "05-box-lid.mcad", "--style", "studio", "--shadows", "off")
render("ao/box-off", "05-box-lid.mcad", "--style", "studio", "--shadows", "off", "--ao", "off")
data["diff"]["ao_box"] = changed("ao/box-on", "ao/box-off")

# 3. ground and background
render("ground/default", "07-mug.mcad", "--style", "studio")
render("ground/off-white", "07-mug.mcad", "--style", "studio", "--ground", "off", "--background", "ffffff")
render("ground/sand-blue", "07-mug.mcad", "--style", "studio", "--ground-color", "d8c8a8", "--background", "eef3f8,9fb3c8")
render("ground/dark", "07-mug.mcad", "--style", "studio", "--theme", "dark")
render("ground/standard-gradient", "07-mug.mcad", "--background", "ffffff,c8ccd2")

# 4. light directions and softness
LIGHTS = [("default", "-40,50"), ("right-low", "60,25"), ("left-low", "-100,20"), ("above", "0,85")]
for n, l in LIGHTS:
    render(f"light/{n}", "05-box-lid.mcad", "--style", "studio", "--light", l)
for s in ["0", "0.35", "1"]:
    render(f"soft/{s}", "02-bracket.mcad", "--style", "studio", "--ao", "off", "--shadow-softness", s, "--light", "-40,40")
render("light/intensity-ambient", "05-box-lid.mcad", "--style", "studio", "--light-intensity", "1.4", "--ambient", "0.2")

# 5. contact sheet, section, perspective
render("sheet/standard", "08-macropad.mcad", "--views", "all", size="1600x1200")
render("sheet/studio", "08-macropad.mcad", "--views", "all", "--style", "studio", size="1600x1200")
render("section/box", "05-box-lid.mcad", "--style", "studio", "--section", "y=20", "--section-flip", "--azimuth", "30", "--elevation", "35")
render("section/persp", "08-macropad.mcad", "--style", "studio", "--perspective", "--azimuth", "-30", "--elevation", "28")
render("section/xray", "05-box-lid.mcad", "--style", "studio", "--xray")
render("section/line", "05-box-lid.mcad", "--style", "studio", "--part", "bin")

# probes: ground behind the bracket's wall (shadow) and the inner corner of the tray (AO)
for n in ["shadow/on", "shadow/off"]:
    data["probe"][n] = luma(n, (560, 330, 640, 400))
for n in ["ao/on", "ao/off", "ao/strong"]:
    data["probe"][n] = luma(n, (0, 0, 900, 675))

# 6. timings (1024x768 unless stated)
T = data["timings"]
for m in MODELS:
    stem = m.removesuffix(".mcad")
    T[stem] = {
        "standard": median_ms(m),
        "studio": median_ms(m, "--style", "studio"),
        "studio_no_shadows": median_ms(m, "--style", "studio", "--shadows", "off"),
        "studio_no_ao": median_ms(m, "--style", "studio", "--ao", "off"),
        "studio_plain": median_ms(m, "--style", "studio", "--shadows", "off", "--ao", "off"),
    }
    if args.before:
        T[stem]["before_standard"] = median_ms(m, binary=args.before)
T["sheet"] = {
    "standard": median_ms("08-macropad.mcad", "--views", "all", size="1600x1200"),
    "studio": median_ms("08-macropad.mcad", "--views", "all", "--style", "studio", size="1600x1200"),
}
T["large"] = {
    "standard": median_ms("05-box-lid.mcad", size="1920x1440"),
    "studio": median_ms("05-box-lid.mcad", "--style", "studio", size="1920x1440"),
}
data["cores"] = os.cpu_count()
data["repeat"] = args.repeat
(OUT / "tmp/timing.png").unlink(missing_ok=True)
(OUT / "data.json").write_text(json.dumps(data, indent=2))
print(f"{OUT / 'data.json'}: {len(data['commands'])} images, timings of {len(T)} cases")
