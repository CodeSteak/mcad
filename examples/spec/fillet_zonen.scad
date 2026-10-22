// Zweck: Zonenblock fillet(r) { körper; zone; … } mit Rasterlinie, Rasterebene und
// Volumenzone, select = "touch", verschachtelte Blöcke mit zwei Radien.
module block(w = 30, d = 20, h = 15) {
    grid g {
        x: links = 0, rechts = w;
        y: vorne = 0, hinten = d;
        z: boden = 0, deckel = h;
    }
    p = [w / 2, d / 2, 0];

    fillet(1) {
        fillet(2) {
            difference() {
                box(g[links, vorne, boden], g[rechts, hinten, deckel]);
                translate(p) cylinder(h = h + 1, r = 4);
            }
            zone(g[rechts, *, deckel], d = 2);     // obere rechte Kante
            translate(p + [0, 0, h]) sphere(6);   // Bohrungskante oben
        }
        zone(g.boden, d = 1);                     // alle Bodenkanten (äußerer Radius)
    }

    translate([0, d + 10, 0]) fillet(1.5, select = "touch") {
        box(g[links, vorne, boden], g[rechts, hinten, deckel]);
        zone(g[links, vorne, *], d = 1);
    }
}

block();
