// Zweck: Stiftebox mit Fächern für den Schreibtisch – Fachgrenzen als indizierte Rasterebenen
// aus Listen (Fachbreiten als Parameter), Taschen als box zwischen Rasterpunkten; vier
// verschachtelte Zonenblöcke: senkrechte Außen- und Taschenkanten (Rasterlinien, Zonen in einer
// for-Schleife), Taschenböden (Platte um die Innenbodenebene) und die Oberkanten hinten (Fase an
// der Deckelebene). Vorderes Fach niedriger; seine Oberkanten bleiben scharf (README: F6).
// Maße in mm.

// Summe der ersten n Einträge einer Liste
function summe(v, n) = n <= 0 ? 0 : v[n - 1] + summe(v, n - 1);

module stiftebox(faecher = [38, 26, 26], tiefe = 70, h = 95, wand = 2.4, boden = 3,
                 vorn_t = 22, vorn_h = 45, ecke = 5, ecke_i = 2) {
    n = len(faecher);
    w = summe(faecher, n) + (n + 1) * wand;
    grid g {
        x: links = 0, rechts = w,
           von = [for (i = [0 : n - 1]) wand + summe(faecher, i) + i * wand],
           bis = [for (i = [0 : n - 1]) wand + summe(faecher, i + 1) + i * wand];
        y: vorne = 0, vorn_i = wand, steg_v = wand + vorn_t, steg_h = 2 * wand + vorn_t,
           hinten_i = tiefe - wand, hinten = tiefe;
        z: unten = 0, boden_i = boden, niedrig = vorn_h, oben = h;
    }

    chamfer(0.6) {
        fillet(1.5) {
          fillet(ecke_i) {
            fillet(ecke) {
                difference() {
                    union() {
                        box(g[links, steg_v, unten], g[rechts, hinten, oben]);
                        box(g[links, vorne, unten], g[rechts, steg_h, niedrig]);
                    }
                    // hohe Fächer hinten
                    for (i = [0 : n - 1])
                        box(g[von[i], steg_h, boden_i], g[bis[i], hinten_i, oben] + [0, 0, 1]);
                    // ein durchgehendes flaches Fach vorn (Radiergummi, Klammern)
                    box(g[von[0], vorn_i, boden_i], g[bis[n - 1], steg_v, niedrig] + [0, 0, 1]);
                }
                zone(g[links, vorne, *], d = 1);
                zone(g[rechts, vorne, *], d = 1);
                zone(g[links, hinten, *], d = 1);
                zone(g[rechts, hinten, *], d = 1);
            }
            // senkrechte Innenecken aller Taschen
            for (i = [0 : n - 1]) {
                zone(g[von[i], steg_h, *], d = 1);
                zone(g[bis[i], steg_h, *], d = 1);
                zone(g[von[i], hinten_i, *], d = 1);
                zone(g[bis[i], hinten_i, *], d = 1);
            }
            zone(g[von[0], vorn_i, *], d = 1);
            zone(g[bis[n - 1], vorn_i, *], d = 1);
            zone(g[von[0], steg_v, *], d = 1);
            zone(g[bis[n - 1], steg_v, *], d = 1);
          }
          zone(g.boden_i, d = 0.5);         // Kanten der Taschenböden
        }
        zone(g.oben, d = 0.5);              // Oberkanten hinten
    }

    anchor fach_mitte = g[von[0], hinten_i, boden_i] + [faecher[0] / 2, -tiefe / 3, 0], dir = [0, 0, 1];
}

box_: stiftebox();
