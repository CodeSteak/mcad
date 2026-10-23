// F9a: ABSTURZ (Segmentation fault, Exit 139) von `cad check`: union(fillet = …) aus Platte,
// Wand und einem Zylinder, der die Wand tangential berührt (Schraubdom an der Wand). Ohne
// fillet oder mit Abstand zur Wand kein Absturz. Im Gehäuse umgangen (Dome frei stehend).
// ERWARTET: kein Absturz
union(fillet = 1) {
    cube([20, 20, 2]);
    cube([2, 20, 10]);
    translate([5.5, 10, 0]) cylinder(h = 10, r = 3.5);
}
