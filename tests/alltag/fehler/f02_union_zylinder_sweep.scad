// F2: Vereinigung eines Zylinders mit einem gekrümmten sweep, dessen Endkappen die
// Zylinderfläche schneiden: „boolean union failed: SM_ERR_NULL_POINTER (1006)“. Liegen die
// Enden tiefer im Zylinder (r = 15 statt 19), geht es. In einem union() { … }-Block erscheint
// dieselbe Meldung zweimal an 1:1 (doppelte Diagnose).
// ERWARTET: fehlerfrei
cylinder(h = 40, r = 20);
sweep(path = [[19, 0, 30], [35, 0, 25], [19, 0, 10]]) circle(3);
