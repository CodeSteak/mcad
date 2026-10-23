// Bauteile des Elektronikgehäuses (per use eingebunden: nur Module und Funktionen sind in der
// Hauptdatei sichtbar). Die Standardwerte kommen aus masse.scad, hier per include.
include <masse.scad>

module schraubdom(h = 20, d = 7, kern = 2.5, tiefe = 12) {
    difference() {
        cylinder(h = h, d = d);
        translate([0, 0, h - tiefe]) cylinder(h = tiefe + 1, d = kern);
    }
    anchor oben = [0, 0, h], dir = [0, 0, 1];
}

// Unterteil: verrundeter Quader, mit shell oben geöffnet, vier Dome in den Innenecken,
// Kabeldurchführung in der linken Stirnwand
module unterteil(l = aussen_l, b = aussen_b, h = hoehe, t = wand, r = ecke, kabel_d = 8) {
    grid g {
        x: links = 0, innen_l = t, mitte = l / 2, innen_r = l - t, rechts = l;
        y: vorne = 0, innen_v = t, mitte_y = b / 2, innen_h = b - t, hinten = b;
        z: boden = 0, innen_b = t, kabel = h / 2, oben = h;
    }
    difference(chamfer = 0.5) {
        union(fillet = 1) {
            shell(t) {
                fillet(t + 1.5) {     // Radius > Wandstärke, sonst hat der Innenradius 0
                    fillet(r) {
                        box(g[links, vorne, boden], g[rechts, hinten, oben]);
                        zone(g[links, vorne, *], d = 1);
                        zone(g[rechts, vorne, *], d = 1);
                        zone(g[links, hinten, *], d = 1);
                        zone(g[rechts, hinten, *], d = 1);
                    }
                    zone(g.boden, d = 1);
                }
                zone(g.oben, d = 1);
            }
            for (i = [0 : 3])
                translate([dom_pos(i).x, dom_pos(i).y, g.innen_b - 0.5])
                    dom[i]: schraubdom(h = h - t + 0.5, d = dom_d, kern = kern_d);
        }
        cylinder(g[links, mitte_y, kabel] - [1, 0, 0], g[innen_l, mitte_y, kabel] + [1, 0, 0], d = kabel_d);
    }
    anchor deckel_auflage = g[links, vorne, oben], dir = [0, 0, 1];
    anchor kabel = g[links, mitte_y, kabel], dir = [-1, 0, 0];
}

// Deckel: Platte mit verrundeten Ecken, Zentrierrand (an den Ecken für die Dome ausgespart),
// Durchgangslöcher über den Domen
module deckel(l = aussen_l, b = aussen_b, t = deckel_t, r = ecke, rand = rand_h, s = spiel) {
    grid g {
        x: links = 0, rand_l = wand + s, rand_r = l - wand - s, rechts = l;
        y: vorne = 0, rand_v = wand + s, rand_h = b - wand - s, hinten = b;
        z: rand_u = -rand, unten = 0, oben = t;
    }
    difference(chamfer = 0.3) {
        union() {
            fillet(1) {
                fillet(r) {
                    box(g[links, vorne, unten], g[rechts, hinten, oben]);
                    zone(g[links, vorne, *], d = 1);
                    zone(g[rechts, vorne, *], d = 1);
                    zone(g[links, hinten, *], d = 1);
                    zone(g[rechts, hinten, *], d = 1);
                }
                zone(g.oben, d = 0.5);
            }
            // Zentrierrand 1,5 mm breit
            difference() {
                box(g[rand_l, rand_v, rand_u], g[rand_r, rand_h, unten] + [0, 0, 0.5]);
                box(g[rand_l, rand_v, rand_u] + [1.5, 1.5, -1], g[rand_r, rand_h, unten] - [1.5, 1.5, -1]);
                for (i = [0 : 3])   // Ecken frei (Dome und verrundete Innenecken des Unterteils)
                    translate([dom_pos(i).x, dom_pos(i).y, 0])
                        cube([2 * (dom_pos(0).x - wand) + 1, 2 * (dom_pos(0).y - wand) + 1, 3 * rand], center = true);
            }
        }
        for (i = [0 : 3])
            translate([dom_pos(i).x, dom_pos(i).y, -1]) cylinder(h = t + 2, d = durch_d);
    }
}

// Linsenkopfschraube M3, vereinfacht: Schaft mit Kerndurchmesser 2,4 (Gewinde nicht modelliert)
module schraube(l = schraube_l) {
    difference() {
        cylinder(h = 2.2, d = 5.5, $fillet = 0.5);
        translate([0, 0, 1.4]) cube([3, 0.6, 2], center = true);
        translate([0, 0, 1.4]) cube([0.6, 3, 2], center = true);
    }
    translate([0, 0, -l]) union() {
        translate([0, 0, 1]) cylinder(h = l - 1 + 0.01, d = 2.4);
        cylinder(h = 1, r1 = 0.6, r2 = 1.2);
    }
}
