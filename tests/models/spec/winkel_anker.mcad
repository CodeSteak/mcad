// Zweck: Anker von außen – zwei benannte Instanzen eines Winkels, Schrauben an den Ankern
// mit rotate(to = dir(k.a)), gedrehte Instanz, Raster aus Ankerkoordinaten.
module winkel(l = 30, b = 20, t = 3) {
    grid g {
        x: innen = t, aussen = 0, ende = l, loch_x = l - 8;
        y: vorne = 0, hinten = b, mitte = b / 2;
        z: unten = 0, oben = l, loch_z = l - 8;
    }
    union() {
        box(g[aussen, vorne, unten], g[ende, hinten, unten] + [0, 0, t]);
        box(g[aussen, vorne, unten], g[innen, hinten, oben]);
    }
    anchor boden_loch = g[loch_x, mitte, unten] + [0, 0, t], dir = [0, 0, 1];
    anchor wand_loch = g[innen, mitte, loch_z], dir = [1, 0, 0];
    anchor spitze = g[ende, hinten, unten];
}

module schraube(l = 10) {
    cylinder(h = 2, d = 6);
    translate([0, 0, -l]) cylinder(h = l, d = 3);
}

a: winkel();
translate([0, 40, 0]) rotate([0, 0, 90]) b: winkel(l = 40);

translate(a.boden_loch) rotate(to = dir(a.boden_loch)) schraube();
translate(a.wand_loch) rotate(to = dir(a.wand_loch)) schraube(8);
translate(b.wand_loch) rotate(to = dir(b.wand_loch)) schraube(8);

// Lineal zwischen den Spitzen beider Winkel
grid m {
    x: von = a.spitze.x, bis = b.spitze.x;
    y: y0 = a.spitze.y;
}
translate(m[von, y0, _]) cube([2, 2, 2]);
