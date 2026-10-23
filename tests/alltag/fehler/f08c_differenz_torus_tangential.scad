// F8c: Zylinder (h 2) minus Torus (Rohrradius 1), der Deck- und Bodenfläche berührt:
// Volumen 0 mm³, Status ok. Erwartet 5654,87 − 394,78 = 5260,08 mm³ (oder eine Fehlermeldung).
// ERWARTET-VOLUMEN: 5260.08 1
difference() {
    cylinder(h = 2, r = 30, center = true);
    torus(20, 1);
}
