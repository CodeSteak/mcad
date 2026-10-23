// Teile des Schlüsselbretts (per use eingebunden). Vorderseite des Bretts ist y = 0, die Wand
// liegt bei y = t; Haken zeigen nach -y.
include <masse.scad>

module brett(l = brett_l, h = brett_h, t = brett_t, n = anzahl, ecke = 8) {
    grid g {
        x: links = 0, rechts = l, haken = [for (i = [0 : n - 1]) haken_x(i)], aufh = [l * 0.15, l * 0.85];
        y: vorne = 0, hinten = t;
        z: unten = 0, reihe = haken_z, aufh_z = h - 32, ablage = h - 12, oben = h;
    }
    difference(chamfer = 0.5) {
        chamfer(1.5) {
            fillet(ecke) {
                box(g[links, vorne, unten], g[rechts, hinten, oben]);
                zone(g[links, *, unten], d = 1);
                zone(g[rechts, *, unten], d = 1);
                zone(g[links, *, oben], d = 1);
                zone(g[rechts, *, oben], d = 1);
            }
            zone(g.vorne, d = 1);
        }
        // Bohrungen für die Hakenzapfen
        for (i = [0 : n - 1])
            cylinder(g[haken[i], vorne, reihe] - [0, 1, 0], g[haken[i], vorne, reihe] + [0, zapfen_l + 1, 0],
                     d = bohrung_d);
        // Schlüssellochaufhängung hinten: Einführloch 10 mm, Schlitz 4,5 mm nach oben,
        // Hinterschnitt 8,5 mm für den Schraubenkopf
        for (i = [0 : 1]) {
            cylinder(g[aufh[i], hinten, aufh_z] + [0, 1, 0], g[aufh[i], hinten, aufh_z] - [0, 7, 0], d = 10);
            box(g[aufh[i], hinten, aufh_z] + [-2.25, 1, 0], g[aufh[i], hinten, aufh_z] + [2.25, -7, 10]);
            box(g[aufh[i], hinten, aufh_z] + [-4.25, -3.5, 0], g[aufh[i], hinten, aufh_z] + [4.25, -7, 10]);
        }
        // Rille oben für Briefe
        box(g[links, vorne, ablage] + [10, 3, 0], g[rechts, hinten, oben] + [-10, -3, 1]);
    }
}

module haken(d = 6, zapfen = zapfen_d, zl = zapfen_l, ausladung = 22, knauf_d = 10) {
    // Zapfen ohne Kehle (sie läge in der Fase der Bohrung); union(fillet) über die fast
    // tangentiale Fuge Zapfen/Bogen bringt die CLI zum Absturz (README: F9)
    cylinder([0, -1, 0], [0, zl, 0], d = zapfen);
    union(fillet = 1) {
        sweep(path = [[0, 0, 0], [0, -ausladung * 0.6, -1], [0, -ausladung, 4], [0, -ausladung - 1, 10]],
              interp = "spline")
            circle(d = d);
        translate([0, -ausladung - 1, 10]) sphere(d = knauf_d);
    }
    anchor spitze = [0, -ausladung - 1, 10], dir = [0, 0, 1];
}
