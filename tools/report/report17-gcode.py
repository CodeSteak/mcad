#!/usr/bin/env python3
"""Picture of the top layer of a PrusaSlicer G-code file (extrusion moves, Bericht 17) and its
key figures as JSON on stdout (layers, estimated time, filament).

    python3 tools/report/report17-gcode.py file.gcode out.png"""
import json, re, sys

from PIL import Image, ImageDraw

src, dst = sys.argv[1], sys.argv[2]
layers = {}  # z -> list of segments
x = y = z = e = 0.0
absolute_e = True
info = {}
for line in open(src, encoding="utf-8", errors="replace"):
    if line.startswith(";"):
        m = re.match(r";\s*(estimated printing time \(normal mode\)|filament used \[mm\]|filament used \[g\]|layer_height|nozzle_diameter)\s*=\s*(.+)", line)
        if m:
            info[m.group(1)] = m.group(2).strip()
        continue
    code = line.split(";")[0].split()
    if not code:
        continue
    if code[0] == "M82":
        absolute_e = True
    elif code[0] == "M83":
        absolute_e = False
    elif code[0] == "G92":
        for w in code[1:]:
            if w[0] == "E":
                e = float(w[1:])
    elif code[0] in ("G0", "G1"):
        nx, ny, nz, ne = x, y, z, None
        for w in code[1:]:
            k, val = w[0], w[1:]
            if k == "X":
                nx = float(val)
            elif k == "Y":
                ny = float(val)
            elif k == "Z":
                nz = float(val)
            elif k == "E":
                ne = float(val)
        extruding = ne is not None and ((ne - e) > 1e-6 if absolute_e else ne > 1e-6)
        if ne is not None and absolute_e:
            e = ne
        if extruding and (nx, ny) != (x, y):
            layers.setdefault(round(nz, 3), []).append((x, y, nx, ny))
        x, y, z = nx, ny, nz

zs = sorted(layers)
top = layers[zs[-1]] if zs else []
pts = [p for s in top for p in ((s[0], s[1]), (s[2], s[3]))]
if pts:
    x0, x1 = min(p[0] for p in pts), max(p[0] for p in pts)
    y0, y1 = min(p[1] for p in pts), max(p[1] for p in pts)
    scale = 1200 / max(x1 - x0, 1e-9)
    w, h = int((x1 - x0) * scale) + 40, int((y1 - y0) * scale) + 40
    img = Image.new("RGB", (w, h), (250, 251, 253))
    d = ImageDraw.Draw(img)
    lw = max(1, int(0.42 * scale))
    for a, b, c, dd in top:
        d.line([(20 + (a - x0) * scale, h - 20 - (b - y0) * scale), (20 + (c - x0) * scale, h - 20 - (dd - y0) * scale)], fill=(214, 120, 40), width=lw)
    img.save(dst)
print(json.dumps({"layers": len(zs), "top_z": zs[-1] if zs else None, "top_moves": len(top), **info}))
