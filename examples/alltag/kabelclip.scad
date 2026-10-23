// Zweck: Kabelhalter zum Aufstecken auf die Schreibtischkante – C-Klammer als extrudiertes
// Profil mit Klemmnoppe, darüber ein Block mit n Kabelkanälen (Einführschlitz von oben).
// Kanäle an indizierten Rasterebenen, Kanalkanten mit difference(fillet = …) verrundet,
// Innenecken der Klammer über Zonen an Rasterlinien. Maße in mm (Platte 19 mm + Spiel).
module kabelclip(dicke = 19, spiel = 0.4, t = 3.5, arm_o = 28, arm_u = 18, n = 4,
                 kabel_d = 6, pitch = 12, block_h = 10) {
    w = n * pitch + 4;
    grid p {   // Profil der Klammer: x = in den Tisch hinein, y = Höhe
        x: aussen = -t, kante = 0, ende_u = arm_u, ende_o = arm_o;
        y: unten = -t, platte_u = 0, platte_o = dicke + spiel, oben = dicke + spiel + t;
    }
    grid g {
        x: aussen = -t, kante = 0, noppe = arm_u - 4, block_e = arm_o;
        y: links = 0, rechts = w, kanal = [for (i = [0 : n - 1]) 2 + pitch / 2 + i * pitch];
        z: unten = -t, platte_u = 0, platte_o = dicke + spiel, oben = dicke + spiel + t,
           achse = dicke + spiel + t + block_h / 2, deckel = dicke + spiel + t + block_h;
    }

    difference(fillet = 0.6) {
        union() {
            fillet(1.5) {
                translate([0, w, 0]) rotate([90, 0, 0]) linear_extrude(w)
                    polygon([p[aussen, unten], p[ende_u, unten], p[ende_u, platte_u], p[kante, platte_u],
                             p[kante, platte_o], p[ende_o, platte_o], p[ende_o, oben], p[aussen, oben]]);
                zone(g[kante, *, platte_u], d = 2);
                zone(g[kante, *, platte_o], d = 2);
            }
            // Klemmnoppe am unteren Arm (drückt von unten gegen die Platte)
            translate(g[noppe, links, platte_u]) rotate([-90, 0, 0])
                cylinder(h = w, r = 1.2);
            box(g[aussen, links, oben] - [0, 0, 1], g[block_e, rechts, deckel]);
        }
        for (i = [0 : n - 1]) {
            cylinder(g[aussen, kanal[i], achse] - [1, 0, 0], g[block_e, kanal[i], achse] + [1, 0, 0],
                     r = kabel_d / 2 + 0.25);
            box(g[aussen, kanal[i], achse] - [1, kabel_d * 0.35, 0],
                g[block_e, kanal[i], deckel] + [1, kabel_d * 0.35, 1]);
        }
    }

    anchor tischkante = g[kante, links, platte_o], dir = [0, 0, 1];
}

clip: kabelclip();
