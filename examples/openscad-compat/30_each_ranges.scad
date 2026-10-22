// Zweck: each, Bereiche mit Schrittweite, verschachtelte und C-artige for-Ausdrücke – Kammstruktur.
xs = [each [0 : 4 : 16], each [24, 30]];
hs = [for (i = 0, h = 4; i < len(xs); i = i + 1, h = h + 2) h];
pairs = [for (i = [0 : 1], j = [i : 2]) [i, j]];
echo(xs = xs, hs = hs, pairs = pairs);
echo(r = [0 : 0.5 : 2], rev = [for (i = [3 : -1 : 0]) i]);
cube([34, 4, 2]);
for (i = [0 : len(xs) - 1]) translate([xs[i], 0, 2]) cube([2, 4, hs[i]]);
