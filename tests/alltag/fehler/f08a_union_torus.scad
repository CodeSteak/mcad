// F8a: Vereinigung Zylinder + Torus, der die Deckfläche schneidet: Ergebnis sind zwei sich
// überlappende Körper (STEP: „2 Körper“), Volumen 6049,65 statt 5852,26 mm³, ohne Meldung.
// ERWARTET-VOLUMEN: 5852.26 1
union() {
    cylinder(h = 2, r = 30);
    translate([0, 0, 2]) torus(20, 1);
}
