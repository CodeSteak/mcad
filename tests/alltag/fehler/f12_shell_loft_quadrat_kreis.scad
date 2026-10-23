// F12: shell eines Lofts vom Quadrat zum Kreis scheitert („SmApiShellBrepFull failed:
// SM_ERR_NULL_POINTER … SmMerge.cpp:672“, der Ersatzweg über Booleans ebenso). Ein Loft
// Kreis → Kreis lässt sich schalen. Umgehung im Pflanztopf: Differenz zweier Lofts.
// ERWARTET: fehlerfrei
shell(2) {
    loft() {
        square(70, center = true);
        translate([0, 0, 90]) circle(50);
    }
    translate([-60, -60, 89.5]) cube([120, 120, 1]);
}
