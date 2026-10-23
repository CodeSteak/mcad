// Zweck: Montagewinkel 60 × 50 mm mit Versteifungsrippe(n) – Schenkel als box zwischen
// Rasterpunkten, Rippen als extrudierte Dreiecke an indizierten Rasterebenen (for), Innenkante
// und Rippenanschlüsse über Zonen an Rasterlinien in einem Block verrundet (Eckübergänge),
// Außenecken der Schenkel über Zonen, Bohrungen (M5) mit difference(chamfer = …). Maße in mm.
module winkel_rippe(a = 60, h = 50, b = 40, t = 5, rippe_t = 4, rippe_l = 30, n_rippen = 1,
                    loch_d = 5.5) {
    grid g {
        x: aussen = 0, innen = t, loch_x = t + (a - t) * 0.6, ende = a;
        y: vorne = 0, loch_l = b * 0.25, loch_r = b * 0.75, hinten = b,
           rippe = [for (i = [0 : n_rippen - 1]) (i + 1) * b / (n_rippen + 1) - rippe_t / 2],
           rippe_e = [for (i = [0 : n_rippen - 1]) (i + 1) * b / (n_rippen + 1) + rippe_t / 2];
        z: unten = 0, innen_z = t, loch_z = t + (h - t) * 0.6, oben = h;
    }

    difference(chamfer = 0.5) {
        fillet(4) {
            fillet(2) {
                union() {
                    box(g[aussen, vorne, unten], g[ende, hinten, innen_z]);     // Grundschenkel
                    box(g[aussen, vorne, unten], g[innen, hinten, oben]);        // Wandschenkel
                    for (i = [0 : n_rippen - 1])
                        translate(g[innen, rippe_e[i], innen_z]) rotate([90, 0, 0]) linear_extrude(rippe_t)
                            polygon([[-1, -1], [rippe_l, -1], [-1, rippe_l]]);
                }
                zone(g[innen, *, innen_z], d = 1);           // Innenkante zwischen den Rippen
                for (i = [0 : n_rippen - 1]) {
                    zone(g[*, rippe[i], innen_z], d = 1);    // Rippe auf dem Grundschenkel
                    zone(g[*, rippe_e[i], innen_z], d = 1);
                    zone(g[innen, rippe[i], *], d = 1);      // Rippe an der Wand
                    zone(g[innen, rippe_e[i], *], d = 1);
                }
            }
            zone(g[ende, vorne, *], d = 1);                  // Ecken des Grundschenkels
            zone(g[ende, hinten, *], d = 1);
            zone(g[*, vorne, oben], d = 1);                  // Ecken des Wandschenkels
            zone(g[*, hinten, oben], d = 1);
        }
        for (y = [g.loch_l, g.loch_r]) {
            cylinder([g.loch_x, y, -1], [g.loch_x, y, t + 1], d = loch_d);
            cylinder([-1, y, g.loch_z], [t + 1, y, g.loch_z], d = loch_d);
        }
    }

    anchor boden_loch = g[loch_x, loch_l, innen_z], dir = [0, 0, 1];
    anchor wand_loch = g[innen, loch_l, loch_z], dir = [1, 0, 0];
}

w: winkel_rippe();
