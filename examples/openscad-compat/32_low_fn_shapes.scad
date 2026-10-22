// Zweck: $fn = 3…12 am Aufruf ergibt Polygone wie in OpenSCAD – Dreieck, Achteck, Pyramide, 2D-Fünfeck.
cylinder(h = 5, r = 6, $fn = 3);
translate([15, 0, 0]) cylinder(h = 5, d = 10, $fn = 8);
translate([30, 0, 0]) cylinder(h = 8, r1 = 5, r2 = 0, $fn = 4);
translate([45, 0, 0]) linear_extrude(height = 3) circle(r = 5, $fn = 5);
translate([0, 15, 0]) cylinder(h = 4, r = 5, center = true, $fn = 12);
