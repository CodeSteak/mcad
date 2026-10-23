// F7: torus() als Zone wählt keine Kante („0 Kanten“), obwohl die obere Kreiskante genau auf
// der Seelenlinie des Torus liegt (auch mit Rohrradius 2). Ein flacher Ring (Differenz zweier
// Zylinder) als Zone wählt sie. Umgehung: Modul ringzone() im Flansch.
// ERWARTET: keine Warnungen
fillet(1) {
    cylinder(h = 10, r = 20);
    translate([0, 0, 10]) torus(20, 1);
}
