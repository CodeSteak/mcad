// Zweck: Strangpress-Kühlkörper 60 × 50 mm mit Rippen – Rippenlage als indizierte Rasterebenen
// aus Listen, Rippen per for als box zwischen Rasterpunkten, Rippenfüße über Zonen an
// Rasterlinien (in einer for-Schleife) verrundet, Rippenköpfe über eine Zone an der Kopfebene
// fast voll gerundet; zwei Befestigungsbohrungen M3 zwischen den Rippen, Anker für die
// Wärmequelle. Maße in mm.
module kuehlkoerper(l = 60, b = 50, basis = 5, h = 25, n = 9, rippe_t = 1.8, fuss_r = 1.5,
                    loch_abstand = 40) {
    pitch = (b - rippe_t) / (n - 1);
    grid g {
        x: vorne = 0, mitte_x = l / 2, hinten = l, loch = [l / 2 - loch_abstand / 2, l / 2 + loch_abstand / 2];
        y: links = 0, mitte = b / 2, rechts = b,
           rippe = [for (i = [0 : n - 1]) i * pitch], rippe_e = [for (i = [0 : n - 1]) i * pitch + rippe_t];
        z: unten = 0, basis_o = basis, kopf = h;
    }

    difference() {
        fillet(rippe_t / 2 - 0.05) {
            fillet(fuss_r) {
                union() {
                    box(g[vorne, links, unten], g[hinten, rechts, basis_o]);
                    for (i = [0 : n - 1])
                        box(g[vorne, rippe[i], basis_o] - [0, 0, 0.5], g[hinten, rippe_e[i], kopf]);
                }
                // Rippenfüße: nur die Kehlen entlang x (eine Zone an der Basisebene erfasst auch die
                // kurzen Außenkanten zwischen den Rippen, die Verrundung scheitert dann, README: F6)
                for (i = [0 : n - 2]) {
                    zone(g[*, rippe_e[i], basis_o], d = 0.5);
                    zone(g[*, rippe[i + 1], basis_o], d = 0.5);
                }
            }
            zone(g.kopf, d = 0.5);
        }
        // Befestigung: Bohrung 3,2 mm in der Mitte zwischen zwei Rippen
        i0 = floor((n - 1) / 2);
        for (x = [g.loch[0], g.loch[1]])
            translate([x, (g.rippe_e[i0] + g.rippe[i0 + 1]) / 2, -1]) cylinder(h = basis + 2, d = 3.2);
    }

    anchor quelle = g[mitte_x, mitte, unten], dir = [0, 0, -1];
}

kk: kuehlkoerper();
