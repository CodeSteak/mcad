// Zweck: Griffschale – shell(t) { körper; zone } mit der Zone an der Rasterebene oben (die
// Oberseite wird geöffnet), Körper mit $fillet verrundet, Kabelausgang per difference,
// Schraubdome an indizierten Rasterebenen mit difference(fillet = …) für die Bohrkanten.
module griffschale(l = 90, b = 34, h = 18, wand = 1.6) {
    grid g {
        x: hinten = 0, vorn = l, dom = [l * 0.2, l * 0.8];
        y: links = 0, rechts = b, mitte = b / 2;
        z: boden = 0, oben = h;
    }

    difference() {
        shell(wand) {
            box(g[hinten, links, boden], g[vorn, rechts, oben], $fillet = 5);
            zone(g.oben, d = 1);    // Deckelseite offen
        }
        translate(g[vorn, mitte, 9]) cube([8, 10, 6], center = true); // Kabelausgang
    }

    // Schraubdome mit verrundeter Bohrkante
    for (i = [0 : 1])
        difference(fillet = 0.5) {
            translate(g[dom[i], mitte, boden]) cylinder(h = h - 2, r = 3.5);
            translate(g[dom[i], mitte, 4]) cylinder(h = h, r = 1.2);
        }

    anchor deckel = g[hinten, links, oben], dir = [0, 0, 1];
}

s: griffschale();
