// Zweck: Türstopper (Keil) – loft durch drei Trapezprofile (glatt, nicht ruled) entlang der
// Länge, mit rotate([90, 0, 90]) liegend ausgerichtet; Griffmulde oben, Querrillen unten
// gegen Rutschen (for), Aufhängeloch hinten; Unterkante des Lofts vorher über eine Zone an
// der Bodenebene gefast. Maße in mm.

// Trapez: unten Breite bu, oben Breite bo, Höhe h (Profil-x = Breite, Profil-y = Höhe)
function trapez(bu, bo, h) = [[-bu / 2, 0], [bu / 2, 0], [bo / 2, h], [-bo / 2, h]];

module tuerstopper(l = 120, b = 44, h_hinten = 32, h_mitte = 14, spitze = 3, rillen = 6,
                   loch_d = 8) {
    grid g {
        x: hinten = 0, loch = 12, mitte = l / 2, vorne = l, rille = [for (i = [0 : rillen - 1]) 25 + i * 14];
        y: links = -b / 2, achse = 0, rechts = b / 2;
        z: boden = 0, loch_z = h_hinten / 2, oben = h_hinten;
    }

    difference() {
        chamfer(1) {
            rotate([90, 0, 90]) loft() {
                polygon(trapez(b, b - 10, h_hinten));
                translate([0, 0, l / 2]) polygon(trapez(b - 2, b - 14, h_mitte));
                translate([0, 0, l]) polygon(trapez(b - 4, b - 16, spitze));
            }
            zone(g.boden, d = 0.5);
        }
        // Querrillen unten
        for (i = [0 : rillen - 1])
            box(g[rille[i], links, boden] - [1.5, 1, 1], g[rille[i], rechts, boden] + [1.5, 1, 1]);
        // Aufhängeloch quer durch das hohe Ende
        cylinder(g[loch, links, loch_z] - [0, 1, 0], g[loch, rechts, loch_z] + [0, 1, 0], d = loch_d);
        // Griffmulde oben (Kugel schneidet eine flache Delle)
        translate(g[loch, achse, oben] + [18, 0, 20.5]) sphere(30);
    }

    anchor spitze = g[vorne, achse, boden], dir = [1, 0, 0];
}

stopper: tuerstopper();
