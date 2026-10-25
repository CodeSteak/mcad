// Teile des Scharniers (per use eingebunden). Achse entlang x durch den Ursprung, Blätter in
// der xy-Ebene (Mittelebene z = 0): Blatt A nach -y, Blatt B nach +y. Die Gelenkaugen teilen
// die Länge l in n gleiche Abschnitte; A trägt die geraden, B die ungeraden Abschnitte.

// Abschnitt i der Augen: [x0, x1], mit axialem Spiel s an jeder inneren Fuge
function auge(i, l, n, s) = [i * l / n + (i == 0 ? 0 : s / 2), (i + 1) * l / n - (i == n - 1 ? 0 : s / 2)];

module blatt(seite = 0, l = 60, breite = 25, t = 2.5, r = 4, n = 5, spiel = 0.4, bohrung = 3,
             loch_d = 3.5, kopf_d = 7) {
    richtung = seite == 0 ? -1 : 1;
    grid g {
        x: anfang = 0, loch = [l * 0.2, l * 0.8], ende = l;
        y: achse = 0, ansatz = richtung * r * 0.75, blatt = richtung * (breite + r), loch_y = richtung * (r + breite * 0.55);
        z: unten = -t / 2, mitte = 0, oben = t / 2;
    }
    difference() {
        difference(chamfer = 0.3) {
            union() {
                fillet(3) {
                    box(g[anfang, ansatz, unten], g[ende, blatt, oben]);   // beginnt im Auge, nicht an der Bohrung
                    zone(g[anfang, blatt, *], d = 1);
                    zone(g[ende, blatt, *], d = 1);
                }
                for (i = [seite : 2 : n - 1])
                    cylinder([auge(i, l, n, spiel)[0], 0, 0], [auge(i, l, n, spiel)[1], 0, 0], r = r);
            }
            // Bohrung für den Stift
            cylinder(g[anfang, achse, mitte] - [1, 0, 0], g[ende, achse, mitte] + [1, 0, 0], d = bohrung);
            // Senkbohrungen (Schraube 3,5 mm), Senkung auf der Seite +z
            for (x = [g.loch[0], g.loch[1]]) {
                translate([x, g.loch_y, -t]) cylinder(h = 2 * t, d = loch_d);
                translate([x, g.loch_y, g.oben - (kopf_d - loch_d) / 2])
                    cylinder(h = (kopf_d - loch_d) / 2 + 0.01, d1 = loch_d, d2 = kopf_d);
            }
        }
        // Freiraum für die Augen des anderen Blatts (axial und radial Spiel s)
        for (i = [1 - seite : 2 : n - 1])
            cylinder([auge(i, l, n, spiel)[0] - spiel, 0, 0], [auge(i, l, n, spiel)[1] + spiel, 0, 0],
                     r = r + spiel);
    }
    anchor achse_anfang = g[anfang, achse, mitte], dir = [1, 0, 0];
}

module stift(l = 60, d = 3, kopf = 1) {
    cylinder([0, 0, 0], [l, 0, 0], d = d);
    cylinder([-kopf, 0, 0], [0, 0, 0], d = d + 2);   // Kopf als axialer Anschlag
}
