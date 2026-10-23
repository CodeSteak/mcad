// Zweck: Stehlagerbock für ein Rillenkugellager 608 (22 × 8 mm) – Grundplatte mit zwei
// Langlöchern (M6, Ausrichten, eigenes Modul), Lagerauge aus Quader und tangential
// anschließendem cylinder(a, b, r) zwischen Rasterpunkten, Lagersitz mit Anschlagschulter. Übergänge Auge/Platte und senkrechte
// Plattenecken über Zonen an Rasterlinien verrundet, Plattenoberkante über eine Zone, Kanten
// von Lagersitz und Langlöchern mit difference(chamfer = …) gefast. Maße in mm.

module lagerbock(l = 80, b = 30, t = 8, achse_h = 30, auge_r = 20, auge_b = 14,
                 lager_d = 22, lager_b = 8, schulter_d = 18, langloch = [6.5, 14]) {
    grid g {
        x: links = 0, loch_l = 10, auge_li = l / 2 - auge_r, mitte = l / 2, auge_re = l / 2 + auge_r,
           loch_r = l - 10, rechts = l;
        y: vorne = 0, auge_v = (b - auge_b) / 2, mitte_y = b / 2, auge_h = (b + auge_b) / 2, hinten = b;
        z: unten = 0, platte = t, achse = achse_h;
    }
    difference(chamfer = 0.5) {
        chamfer(0.6) {
            fillet(3) {
                fillet(5) {
                    union() {
                        box(g[links, vorne, unten], g[rechts, hinten, platte]);
                        box(g[auge_li, auge_v, unten], g[auge_re, auge_h, achse]);
                        cylinder(g[mitte, auge_v, achse], g[mitte, auge_h, achse], r = auge_r);
                    }
                    zone(g[links, vorne, *], d = 1);
                    zone(g[rechts, vorne, *], d = 1);
                    zone(g[links, hinten, *], d = 1);
                    zone(g[rechts, hinten, *], d = 1);
                }
                // Kehlen zwischen Auge und Platte (Rasterlinien entlang x und y)
                zone(g[*, auge_v, platte], d = 1);
                zone(g[*, auge_h, platte], d = 1);
                zone(g[auge_li, *, platte], d = 1);
                zone(g[auge_re, *, platte], d = 1);
            }
            zone(g.platte, d = 0.5);       // Oberkante der Platte
        }
        // Lagersitz von vorn, Schulter hinten
        cylinder(g[mitte, auge_v, achse] - [0, 1, 0], g[mitte, auge_v, achse] + [0, lager_b, 0], d = lager_d);
        cylinder(g[mitte, auge_v, achse], g[mitte, auge_h, achse] + [0, 1, 0], d = schulter_d);
        // Langlöcher
        for (x = [g.loch_l, g.loch_r])
            hull_langloch([x, g.mitte_y, -1], langloch[0], langloch[1], t + 2);
    }

    anchor lager = g[mitte, auge_v, achse], dir = [0, -1, 0];
    anchor fuss_l = g[loch_l, mitte_y, platte], dir = [0, 0, 1];
}

// Langloch quer zur Platte (in y): zwei Zylinder und ein Quader, Länge s zwischen den Mitten
module hull_langloch(p, d, s, h) {
    translate(p) {
        translate([0, -s / 2 + d / 2, 0]) cylinder(h = h, d = d);
        translate([0, s / 2 - d / 2, 0]) cylinder(h = h, d = d);
        translate([-d / 2, -s / 2 + d / 2, 0]) cube([d, s - d, h]);
    }
}

bock: lagerbock();
