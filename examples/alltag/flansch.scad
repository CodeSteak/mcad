// Zweck: Flanschplatte mit Bund und Lochkreis (DN 25-ähnlich, 6 × M10) – Querschnitt als
// Polygon aus einem 2D-Raster, rotate_extrude; Übergang Bund/Scheibe über eine Ringzone
// (eigenes Modul als Zone) verrundet, Bohrungskanten und Außenkanten über Volumenzonen gefast,
// Durchgangslöcher im Lochkreis per for/rotate und Dichtnut mit difference(chamfer = …).
// Maße in mm. (torus() als Zone wählt keine Kanten, siehe README: F7.)

// Zone: flacher Ring um den Kreis mit Radius r in Höhe z
module ringzone(r, z, d = 2) {
    translate([0, 0, z]) difference() {
        cylinder(h = d, r = r + d / 2, center = true);
        cylinder(h = 2 * d, r = r - d / 2, center = true);
    }
}
module flansch(d_aussen = 120, t = 12, d_bund = 60, h_bund = 28, d_bohrung = 32,
               lk = 90, n = 6, d_loch = 11, nut_t = 1.5) {
    // Querschnitt: x = Radius, y = Höhe
    grid p {
        x: bohrung = d_bohrung / 2, bund = d_bund / 2, aussen = d_aussen / 2;
        y: unten = 0, scheibe = t, bund_o = h_bund;
    }
    grid g {
        x: achse = 0, bund = d_bund / 2, lochkreis = lk / 2, aussen = d_aussen / 2;
        z: unten = 0, scheibe = t, bund_o = h_bund;
    }

    difference(chamfer = 0.8) {
        chamfer(1.5) {
            fillet(4) {
                rotate_extrude()
                    polygon([p[bohrung, unten], p[aussen, unten], p[aussen, scheibe], p[bund, scheibe],
                             p[bund, bund_o], p[bohrung, bund_o]]);
                ringzone(g.bund, g.scheibe);                        // Kehle Bund/Scheibe
            }
            cylinder(h = 3 * h_bund, r = d_bohrung / 2 + 1, center = true);  // beide Bohrungskanten
            ringzone(g.aussen, g.scheibe);                                // Außenkante oben
            ringzone(g.bund, g.bund_o);                                   // Bundkante oben
        }
        for (i = [0 : n - 1])
            rotate([0, 0, i * 360 / n + 30]) translate([g.lochkreis, 0, -1]) cylinder(h = t + 2, d = d_loch);
        // Dichtnut auf der Unterseite
        translate([0, 0, -1]) difference() {
            cylinder(h = nut_t + 1, r = d_bohrung / 2 + 9);
            translate([0, 0, -1]) cylinder(h = nut_t + 3, r = d_bohrung / 2 + 5);
        }
    }

    anchor oben = g[achse, _, bund_o], dir = [0, 0, 1];
    anchor dichtflaeche = g[achse, _, unten], dir = [0, 0, -1];
}

f: flansch();
