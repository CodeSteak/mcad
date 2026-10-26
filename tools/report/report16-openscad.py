#!/usr/bin/env python3
"""Comparison with OpenSCAD 2021.01 for test report 16: text() with the bundled font files
(OpenSCAD finds them through a fontconfig file that lists only crates/cad-font/fonts) and
import() of SVG files. Bounds (xy) and area: OpenSCAD's 2D result exported as SVG (polygons),
ours from `cad measure --bounds --volume` of a 1 mm extrusion.

    python3 tools/report/report16-openscad.py OUT_DIR CAD_BINARY

Writes OUT_DIR/openscad.tsv: kind, name, code, then minx miny maxx maxy area for OpenSCAD and
for cad (``nan`` if a run failed)."""
import json, math, os, re, subprocess, sys, tempfile

out, cad = sys.argv[1], sys.argv[2]
root = os.path.abspath(os.path.join(os.path.dirname(__file__), "../.."))
fonts = os.path.join(root, "crates/cad-font/fonts")
work = os.path.join(out, "openscad")
os.makedirs(work, exist_ok=True)
conf = os.path.join(work, "fonts.conf")
with open(conf, "w") as f:
    f.write(f'<?xml version="1.0"?>\n<!DOCTYPE fontconfig SYSTEM "fonts.dtd">\n<fontconfig><dir>{fonts}</dir>'
            f'<cachedir>{work}/fc-cache</cachedir></fontconfig>\n')
env = dict(os.environ, FONTCONFIG_FILE=conf, CAD_NO_DAEMON="1")


def osc_measure(code, name):
    scad = os.path.join(work, f"{name}.scad")
    svg = os.path.join(work, f"{name}.svg")
    with open(scad, "w") as f:
        f.write(code + "\n")
    r = subprocess.run(["openscad", "-o", svg, scad], env=env, capture_output=True, text=True)
    if r.returncode != 0 or not os.path.exists(svg):
        return [math.nan] * 5
    txt = open(svg).read()
    polys = []
    for d in re.findall(r'<path d="([^"]*)"', txt, re.S):
        for sub in re.split(r"[zZ]", d):
            pts = [(float(a), -float(b)) for a, b in re.findall(r"(-?[\d.e+-]+),(-?[\d.e+-]+)", sub)]
            if len(pts) >= 3:
                polys.append(pts)
    xs = [p[0] for q in polys for p in q]
    ys = [p[1] for q in polys for p in q]
    area = abs(sum(sum(q[i][0] * q[(i + 1) % len(q)][1] - q[(i + 1) % len(q)][0] * q[i][1] for i in range(len(q))) / 2 for q in polys))
    return [min(xs), min(ys), max(xs), max(ys), area]


def cad_measure(code, name):
    scad = os.path.join(work, f"{name}.cad.scad")
    with open(scad, "w") as f:
        f.write(f"linear_extrude(1) {{ {code} }}\n")
    r = subprocess.run([cad, "measure", scad, "--bounds", "--volume", "--json"], env=env, capture_output=True, text=True)
    try:
        d = json.loads(r.stdout)
        b = d["bounds"]["bounds"]
        return [b["min"][0], b["min"][1], b["max"][0], b["max"][1], d["volume"]["volume"]]
    except Exception:
        return [math.nan] * 5


cases = []
for font, text, args in [
    ("Inter", "H", ""), ("Inter:style=Bold", "H", ""), ("JetBrains Mono", "H", ""), ("JetBrains Mono:style=Bold", "Hamburg", ""),
    ("Allerta Stencil", "H", ""), ("Inter", "AV", ""), ("Inter", "O", ""), ("Inter", "Hello World", ""),
    ("Inter", "Größe", ""), ("Allerta Stencil", "ZUTRITT", ""), ("JetBrains Mono", "->", ""), ("Inter", "fi", ""),
    ("Inter", "H", ', halign = "center", valign = "center"'), ("Inter", "Hg", ', valign = "bottom"'),
    ("Inter", "Hg", ', valign = "top"'), ("Inter", "HH", ", spacing = 2"), ("Inter", "HH", ', direction = "ttb"'),
    ("Inter", "H", ', halign = "right"'), ("Inter", "Text", ", size = 25"),
]:
    code = f'text("{text}", size = 10, font = "{font}"{args});' if "size" not in args else f'text("{text}", font = "{font}"{args});'
    cases.append(("text", f"{font} {text!r}{args}", code))
for s in ["logo", "blatt", "haus", "welle"]:
    p = os.path.join(root, "tests/models/svg", f"{s}.svg")
    cases.append(("svg", f"{s}.svg", f'import("{p}", dpi = 96);'))
units = os.path.join(work, "units")
os.makedirs(units, exist_ok=True)
for name, attrs in [("mm-viewbox", 'width="100mm" height="50mm" viewBox="0 0 200 100"'),
                    ("px-viewbox", 'width="100px" height="50px" viewBox="0 0 100 50"'),
                    ("in-viewbox", 'width="1in" height="0.5in" viewBox="0 0 100 50"'),
                    ("ohne-viewbox", 'width="100" height="50"')]:
    p = os.path.join(units, f"{name}.svg")
    with open(p, "w") as f:
        f.write(f'<svg xmlns="http://www.w3.org/2000/svg" {attrs}><rect x="10" y="5" width="20" height="10"/>'
                '<circle cx="50" cy="20" r="8"/></svg>\n')
    cases.append(("einheit", name, f'import("{p}", dpi = 96);'))

with open(os.path.join(out, "openscad.tsv"), "w") as f:
    for i, (kind, name, code) in enumerate(cases):
        o = osc_measure(code, f"c{i}")
        c = cad_measure(code, f"c{i}")
        f.write("\t".join([kind, name, code.replace(root + "/", "")] + [repr(x) for x in o + c]) + "\n")
print(f"{len(cases)} Vergleiche -> {out}/openscad.tsv")
