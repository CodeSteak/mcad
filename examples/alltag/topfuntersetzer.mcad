// Zweck: Topfuntersetzer Ø 180 mm – drei konzentrische Ringe (rotate_extrude eines Rechtecks,
// Radien aus einer Liste), Speichen per for/rotate, Mittelscheibe mit Lochkreis; Oberkanten
// über eine Zone an der Deckebene gefast (Ringe und Speichen in einem Block), drei
// Gummifuß-Mulden unten. Maße in mm.
module topfuntersetzer(d = 180, h = 8, steg = 6, ringe = [36, 62, 87], speichen = 8, mitte_r = 20,
                       fuss_d = 10) {
    grid g {
        x: achse = 0, ring = ringe, aussen = d / 2;
        z: unten = 0, mulde = 1.5, oben = h;
    }
    difference() {
        chamfer(0.8) {
            union() {
                for (i = [0 : len(ringe) - 1])
                    rotate_extrude() translate([g.ring[i] - steg / 2, 0]) square([steg, h]);
                for (i = [0 : speichen - 1])
                    rotate([0, 0, i * 360 / speichen + 360 / speichen / 2])
                        box([mitte_r - 1, -steg / 2, 0], [g.ring[len(ringe) - 1], steg / 2, h]);
                cylinder(h = h, r = mitte_r);
            }
            zone(g.oben, d = 0.5);
        }
        // Lochkreis in der Mittelscheibe (Dampf, Material sparen)
        for (i = [0 : 5]) rotate([0, 0, i * 60]) translate([mitte_r * 0.55, 0, -1]) cylinder(h = h + 2, d = 6);
        // Mulden für Klebe-Gummifüße auf dem äußeren Ring
        for (i = [0 : 2]) rotate([0, 0, i * 120]) translate([g.ring[len(ringe) - 1], 0, -1])
            cylinder(h = g.mulde + 1, d = fuss_d);
    }
    anchor auflage = g[achse, _, oben], dir = [0, 0, 1];
}

u: topfuntersetzer();
