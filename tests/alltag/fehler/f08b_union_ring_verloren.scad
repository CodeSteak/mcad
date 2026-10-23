// F8b: Vereinigung Zylinder + rotate_extrude eines Kreises (Ring, halb eingetaucht): der Ring
// fehlt im Ergebnis (Volumen 5654,87 = nur der Zylinder, Schnittbild ohne Ring), ohne Meldung.
// ERWARTET-VOLUMEN: 5852.26 1
union() {
    cylinder(h = 2, r = 30);
    translate([0, 0, 2]) rotate_extrude() translate([20, 0]) circle(1);
}
