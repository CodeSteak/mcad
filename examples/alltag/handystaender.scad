// Zweck: Handyständer für den Schreibtisch – Seitenprofil als Polygon aus einem 2D-Raster,
// linear_extrude über die Breite, dreieckiges Fenster (Eckpunkte über eine Funktion für den
// Schnitt zweier Geraden), Kabelloch mit Kanal unter dem Boden. Die Innenecken von Lippe und
// Ablage und die Lippenoberkante werden über Zonen an Rasterlinien verrundet (zwei Radien).
// Maße in mm; Telefon bis 12 mm dick, Neigung a der Lehne gegen die Waagrechte.

// Schnittpunkt der Geraden p1 + s·d1 und p2 + u·d2 (2D)
function schnitt(p1, d1, p2, d2) =
    let (den = d1.x * d2.y - d1.y * d2.x,
         s = ((p2.x - p1.x) * d2.y - (p2.y - p1.y) * d2.x) / den)
    p1 + s * d1;

module handystaender(w = 70, a = 65, lehne = 95, t = 5, lippe = 12, ablage = 14, kabel_d = 9) {
    x0 = t + ablage;                                   // Beginn der Lehne
    d = [cos(a), sin(a)];                              // Richtung der Lehne
    n = [sin(a), -cos(a)];                             // Normale der Lehne nach hinten unten
    top = [x0 + lehne * cos(a), t + lehne * sin(a)];   // vordere Oberkante der Lehne
    fuss = top.x + t * n.x + 18;                       // hinteres Ende des Bodens
    e = (top + t * n - [fuss, t]) / norm(top + t * n - [fuss, t]);  // Richtung der Strebe

    // Seitenprofil (x nach hinten, y nach oben)
    grid p {
        x: vorn = 0, lippe_i = t, lehne_u = x0, oben_v = top.x, oben_h = top.x + t * n.x, hinten = fuss;
        y: boden = 0, sitz = t, lippe_o = t + lippe, oben_u = top.y + t * n.y, oben = top.y;
    }
    // Dasselbe im Raum (y = Breite) für Zonen und Bohrungen
    grid g {
        x: vorn = 0, lippe_i = t, lehne_u = x0, loch = t + ablage / 2, hinten = fuss;
        y: links = 0, mitte = w / 2, rechts = w;
        z: boden = 0, sitz = t, lippe_o = t + lippe;
    }

    // Fenster: Dreieck zwischen Lehne, Boden und Strebe, je um die Wandstärke t eingerückt
    l1 = p[lehne_u, sitz] + t * n;
    l3 = [fuss, t] + t * [-e.y, e.x];
    fa = schnitt(l1, d, [0, 2 * t], [1, 0]);
    fb = schnitt([0, 2 * t], [1, 0], l3, e);
    fc = schnitt(l1, d, l3, e);

    fillet(1.5) {
        fillet(3) {
            difference() {
                translate([0, w, 0]) rotate([90, 0, 0]) linear_extrude(w)
                    polygon([p[vorn, boden], p[hinten, boden], p[hinten, sitz], p[oben_h, oben_u],
                             p[oben_v, oben], p[lehne_u, sitz], p[lippe_i, sitz], p[lippe_i, lippe_o],
                             p[vorn, lippe_o]]);
                translate([0, w + 1, 0]) rotate([90, 0, 0]) linear_extrude(w + 2) polygon([fa, fb, fc]);
                // Kabel: senkrechtes Loch in der Ablage und Kanal unter dem Boden nach hinten
                translate(g[loch, mitte, boden] - [0, 0, 1]) cylinder(h = t + 2, d = kabel_d);
                box(g[loch, mitte, boden] - [0, kabel_d / 2, 1], g[hinten, mitte, boden] + [1, kabel_d / 2, 3]);
            }
            zone(g[lippe_i, *, sitz], d = 2);   // Innenecke Lippe/Ablage
            zone(g[lehne_u, *, sitz], d = 2);   // Innenecke Ablage/Lehne
        }
        zone(g[vorn, *, lippe_o], d = 2);       // Oberkante der Lippe außen
        zone(g[lippe_i, *, lippe_o], d = 2);    // Oberkante der Lippe innen
    }

    anchor ablage_mitte = g[loch, mitte, sitz], dir = [0, 0, 1];
}

staender: handystaender();
