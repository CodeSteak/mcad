// F5: Fase über eine Zone an der Oberkante eines Kastens mit verrundeten senkrechten Ecken
// (r 6, Wand 3): Status ok, 0 Fehler, aber Volumen 0 mm³ – die Hülle ist offen (STEP mit
// 26 Flächen). Mit fillet(2) statt fillet(6) oder chamfer(0.4) stimmt das Volumen
// (~20 000 mm³). Ein Kernel-Ergebnis ohne geschlossene Hülle müsste ein Fehler sein.
// Status (Bericht 13): behoben – die Fasen der inneren und äußeren Kante stoßen an den Ecken
// zusammen (Wand an der Ecke 1,76 mm < 0,8 + 0,8·√2); statt Volumen 0 ohne Meldung jetzt ein
// Fehler. chamfer(0.7) geht.
// SOLL-FEHLER: not closed
grid g {
    x: l = 0, r = 40;
    y: v = 0, h = 30;
    z: u = 0, o = 50;
}
chamfer(0.8) {
    fillet(6) {
        difference() {
            box(g[l, v, u], g[r, h, o]);
            translate([3, 3, 3]) cube([34, 24, 50]);
        }
        zone(g[l, v, *], d = 1);
        zone(g[r, v, *], d = 1);
        zone(g[l, h, *], d = 1);
        zone(g[r, h, *], d = 1);
    }
    zone(g.o, d = 0.5);
}
