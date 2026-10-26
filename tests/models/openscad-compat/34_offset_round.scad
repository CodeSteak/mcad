// Zweck: offset(r) – Abrunden nach außen und innen an konvexen und konkaven Ecken, Kreis, Polygon.
module l_form() polygon([[0, 0], [20, 0], [20, 8], [8, 8], [8, 20], [0, 20]]);

linear_extrude(2) offset(r = 2) square(10);                     // außen: Ecken werden Bögen
translate([20, 0, 0]) linear_extrude(2) offset(r = -2) square(10);  // innen: Ecken bleiben spitz
translate([40, 0, 0]) linear_extrude(2) offset(3) circle(r = 5);    // positional = r
translate([60, 0, 0]) linear_extrude(2) offset(r = 1.5) l_form();   // konkave Ecke bleibt spitz
translate([90, 0, 0]) linear_extrude(2) offset(r = -1.5) l_form();  // konkave Ecke wird rund
// Öffnen (erst innen, dann außen): spitze Ecken werden rund, schmale Stege verschwinden
translate([0, 40, 0]) linear_extrude(3) offset(r = 2) offset(r = -2) union() {
    square([30, 12]);
    translate([12, 0]) square([3, 30]);
    translate([0, 20]) square([1.5, 10]);
}
