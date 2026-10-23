// F13 (Meldung): shell(2) eines Körpers, dessen Bodenkanten mit r 2 verrundet sind (Innenradius
// 0): vorher nur „SmApiShellBrepFull failed: SM_ERR (1001)“.
// Status (Bericht 13): behoben – die Meldung nennt jetzt die Ursache (Verrundungsradius ≤
// Wandstärke, die Innenfläche entartet); mit fillet(3) geht es.
// SOLL-FEHLER: radius of a convex rounding
grid g { z: o = 30; }
shell(2) {
    fillet(2) {
        box([0, 0, 0], [50, 40, 30]);
        zone(g[*, *, 0], d = 1);
    }
    zone(g.o, d = 1);
}
