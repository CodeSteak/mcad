// F8c: Zylinder (h 2) minus Torus (Rohrradius 1), der Deck- und Bodenfläche berührt:
// Volumen 0 mm³, Status ok. Erwartet 5654,87 − 394,78 = 5260,08 mm³ (oder eine Fehlermeldung).
// Status (Bericht 13): behoben als Meldung – der Torus berührt Deck- und Bodenfläche
// tangential, das Ergebnis ist nicht mannigfaltig (Volumen 5260 mm³, aber kein geschlossener
// Körper); vorher Volumen 0 ohne Meldung.
// SOLL-FEHLER: not a closed solid
difference() {
    cylinder(h = 2, r = 30, center = true);
    torus(20, 1);
}
