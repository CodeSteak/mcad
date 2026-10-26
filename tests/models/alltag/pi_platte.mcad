// Zweck: Montageplatte für einen Raspberry Pi (Lochbild 58 × 49 mm, M2.5) – Lochbild als
// Rasterebenen, Abstandshalter als eigenes Modul mit Anker, in der Schleife als benannte
// Instanzen dom[i] gesetzt und mit union(fillet = …) angesetzt; Lüftungsschlitze (for),
// Befestigungslöcher M4 mit Senkung (Kegel), Plattenecken über Zonen verrundet.
// Platine als zweites Bauteil (Dummy 85 × 56 × 1,6) auf dem Anker der Platte. Maße in mm.
module abstandshalter(h = 6, d = 6, kern = 2.1) {
    difference() {
        cylinder(h = h, d = d);
        translate([0, 0, 1]) cylinder(h = h, d = kern);   // Kernloch für M2.5 (selbstschneidend)
    }
    anchor oben = [0, 0, h], dir = [0, 0, 1];
}

module pi_platte(l = 100, b = 72, t = 3, ecke = 6, hoehe = 6, rand_x = 10, rand_y = 8,
                 schlitze = 6) {
    grid g {
        x: links = 0, rechts = l, pi_x0 = rand_x, loch_l = rand_x + 3.5, loch_r = rand_x + 3.5 + 58,
           befest = [5, l - 5], schlitz = [for (i = [0 : schlitze - 1]) rand_x + 18 + i * 7];
        y: vorne = 0, hinten = b, pi_y0 = rand_y, loch_v = rand_y + 3.5, loch_h = rand_y + 3.5 + 49,
           befest_y = [5, b - 5], schlitz_v = rand_y + 14, schlitz_h = rand_y + 42;
        z: unten = 0, oben = t;
    }

    difference() {
        union(fillet = 1) {
            fillet(ecke) {
                box(g[links, vorne, unten], g[rechts, hinten, oben]);
                zone(g[links, vorne, *], d = 1);
                zone(g[rechts, vorne, *], d = 1);
                zone(g[links, hinten, *], d = 1);
                zone(g[rechts, hinten, *], d = 1);
            }
            for (i = [0 : 3])
                // Ebenennamen sind keine Werte: Auswahl über die Koordinaten (g.name als Zahl)
                translate([i % 2 == 0 ? g.loch_l : g.loch_r, i < 2 ? g.loch_v : g.loch_h, g.oben - 0.5])
                    dom[i]: abstandshalter(h = hoehe + 0.5);
        }
        // Lüftungsschlitze unter dem Prozessor
        for (i = [0 : schlitze - 1])
            hull_schlitz(g[schlitz[i], schlitz_v, unten] - [0, 0, 1], g[schlitz[i], schlitz_h, unten] - [0, 0, 1],
                         3, t + 2);
        // Befestigung M4 mit 90°-Senkung
        for (x = [g.befest[0], g.befest[1]], y = [g.befest_y[0], g.befest_y[1]]) {
            translate([x, y, -1]) cylinder(h = t + 2, d = 4.5);
            translate([x, y, t - 2]) cylinder(h = 2.01, r1 = 2.25, r2 = 4.25);
        }
    }

    anchor pi = g[pi_x0, pi_y0, oben] + [0, 0, hoehe], dir = [0, 0, 1];
    anchor schraube = dom[0].oben, dir = [0, 0, 1];
}

// Langloch zwischen zwei Punkten (Mitten), Breite w, Höhe h
module hull_schlitz(a, e, w, h) {
    translate(a) cylinder(h = h, d = w);
    translate(e) cylinder(h = h, d = w);
    translate(a - [w / 2, 0, 0]) cube([w, e.y - a.y, h]);
}

// Platinen-Dummy Raspberry Pi (85 × 56 × 1,6, Löcher 2,7 im Lochbild 58 × 49)
module pi_platine() {
    difference() {
        cube([85, 56, 1.6], $fillet = 0.5);
        for (x = [3.5, 61.5], y = [3.5, 52.5]) translate([x, y, -1]) cylinder(h = 4, d = 2.7);
    }
    color("green") translate([25, 18, 1.6]) cube([15, 15, 2.4]);      // SoC
    color("gray") translate([85 - 21, 2, 1.6]) cube([21, 53, 16]);     // USB/LAN-Block
}

platte: pi_platte();
translate(platte.pi) pi: pi_platine();
