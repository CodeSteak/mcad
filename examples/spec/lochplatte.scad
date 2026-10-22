// Zweck: Lochplatte mit indizierten Rasterebenen (Listen), Bohrungen an Rasterpunkten,
// an Rasterpunkten platziertes zweites Raster und difference(fillet = 1) für die Bohrkanten.
module lochplatte(n = 4, pitch = 12, t = 4, rand = 6) {
    w = 2 * rand + (n - 1) * pitch;
    grid g {
        x: links = 0, rechts = w, loch = [for (i = [0 : n - 1]) rand + i * pitch];
        y: vorne = 0, hinten = 2 * rand, mitte = rand;
        z: boden = 0, oben = t;
    }

    difference(fillet = 1) {
        box(g[links, vorne, boden], g[rechts, hinten, oben]);
        for (i = [0 : n - 1])
            translate(g[loch[i], mitte, boden]) translate([0, 0, -1]) cylinder(h = t + 2, d = 4);
    }

    // Raster für die Lasche, am rechten Rand platziert
    translate(g[rechts, vorne, boden]) grid l {
        x: anfang = 0, ende = 10;
        y: v = 0, h = 2 * rand;
    }
    box(l[anfang, v, _], l[ende, h, _] + [0, 0, t / 2]);

    anchor erstes_loch = g[loch[0], mitte, oben], dir = [0, 0, 1];
    anchor lasche = l[ende, h, _];
}

p: lochplatte();
translate(p.erstes_loch) rotate(to = dir(p.erstes_loch)) cylinder(h = 6, d = 3.5);
