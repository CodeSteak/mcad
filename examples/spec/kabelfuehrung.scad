// Zweck: Kabelführung – sweep eines U-Profils (zentriert um seinen Ursprung) entlang eines
// Spline-Pfads aus Rasterpunkten und Literalen; ein zweiter Kanal folgt einem linearen Pfad.
// Halteklötze an den Rasterpunkten.
grid g {
    x: start = 0, mitte = 40, ende = 90;
    y: vorne = 0, versatz = 25;
    z: unten = 0, oben = 20;
}

module u_profil(b = 10, h = 8, t = 1.5) {
    // Bounding-Box-Mitte im Ursprung (SMLib schiebt die Profilmitte auf den Pfad)
    translate([-b / 2, -h / 2])
        polygon([[0, 0], [b, 0], [b, h], [b - t, h], [b - t, t], [t, t], [t, h], [0, h]]);
}

sweep(path = [g[start, vorne, unten], [20, 5, 12], g[mitte, versatz, oben], g[ende, versatz, unten]], interp = "spline")
    u_profil();

translate([0, 50, 0])
    sweep(path = [[0, 0, 0], [50, 0, 0], [50, 30, 0]], interp = "linear")
        u_profil(b = 8, h = 6);

for (p = [g[start, vorne, unten], g[ende, versatz, unten]])
    translate(p - [6, 6, 10]) cube([12, 12, 6]);
