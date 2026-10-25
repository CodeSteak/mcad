// Zweck: Becher mit Henkel (randvoll ca. 370 ml) – konischer Rotationskörper (rotate_extrude), unten
// über eine Zone an der Bodenebene verrundet, mit shell oben geöffnet, Trinkrand über eine
// zweite Zone verrundet; Henkel als sweep eines Kreises entlang eines Spline-Pfads (Enden
// radial in der Wand), mit union(fillet = …) angesetzt. Maße in mm.

// Radius der Außenwand in Höhe z (linear von unten nach oben)
function radius_bei(z, ru, ro, h) = ru + (ro - ru) * z / h;

module becher(ru = 36, ro = 41, h = 95, wand = 2.8, boden_r = 6, henkel_d = 9) {
    // Profil des Rotationskörpers: x = Radius, y = Höhe
    grid p {
        x: achse = 0, unten = ru, oben = ro;
        y: boden = 0, rand = h;
    }
    // Raum: Zonen und Henkel
    grid g {
        x: achse = 0;
        z: boden = 0, henkel_u = 22, henkel_o = h - 18, rand = h;
    }
    r1 = radius_bei(g.henkel_u, ru, ro, h);
    r2 = radius_bei(g.henkel_o, ru, ro, h);
    weite = 30;    // Abstand des Henkels von der Wand

    union(fillet = 1.5) {
        fillet(1.2) {
            shell(wand) {
                fillet(boden_r) {
                    rotate_extrude()
                        polygon([p[achse, boden], p[unten, boden], p[oben, rand], p[achse, rand]]);
                    zone(g.boden, d = 1);
                }
                zone(g.rand, d = 1);     // Oberseite öffnen
            }
            zone(g.rand, d = 1);         // Trinkrand innen und außen verrunden
        }
        sweep(path = [[r2 - wand / 2, 0, g.henkel_o], [r2 + 4, 0, g.henkel_o + 0.5],
                      [r2 + weite * 0.6, 0, g.henkel_o + 3],
                      [ro + weite, 0, (g.henkel_u + g.henkel_o) / 2 + 6],
                      [r1 + weite * 0.6, 0, g.henkel_u - 3], [r1 + 4, 0, g.henkel_u - 0.5],
                      [r1 - wand / 2, 0, g.henkel_u]],
              interp = "spline")
            circle(d = henkel_d);   // elliptisch ginge nur als Polygon (README: F1/F3)
    }

    anchor rand_mitte = g[achse, _, rand], dir = [0, 0, 1];
}

b: becher();
