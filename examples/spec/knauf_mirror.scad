// Zweck: Knauf – 2D-Raster für ein Splineprofil, rotate_extrude, torus, difference(fillet = …)
// an der Bohrung; gespiegelte Kopie an einer Ebene eines skalierten Rasters.
module knauf(r = 15, h = 20) {
    grid p {
        x: achse = 0, taille = r * 0.6, aussen = r;
        y: fuss = 0, mitte = h / 2, kopf = h;
    }
    difference(fillet = 0.5) {
        rotate_extrude()
            spline([p[achse, fuss], p[aussen, fuss], p[taille, mitte], p[aussen, kopf], p[achse, kopf]]);
        translate([0, 0, -1]) cylinder(h = h / 2, d = 6);
    }
    translate([0, 0, p.kopf]) torus(r * 0.8, 1.5);
    anchor oben = [0, 0, p.kopf], dir = [0, 0, 1];
}

scale(2) grid s {
    x: spiegel = 20;
}

k: knauf();
mirror(s.spiegel) knauf(r = 10);
translate(k.oben) rotate(to = dir(k.oben)) cylinder(h = 3, r = 2);
