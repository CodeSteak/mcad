// Zweck: Sechskantmutter M5 – $fn = 6 am Aufruf erzeugt das Sechseck, die Bohrung ist rund.
s = 8;            // Schlüsselweite
m = 4;            // Höhe
d_hole = 5;
e = s / cos(30);  // Eckmaß
echo(eckmass = e);
difference() {
    cylinder(h = m, d = e, $fn = 6);
    translate([0, 0, -1]) cylinder(h = m + 2, d = d_hole);
}
// Unterlegscheibe daneben, Quadratmutter ($fn = 4)
translate([15, 0, 0]) difference() {
    cylinder(h = m, r = 5, $fn = 4);
    translate([0, 0, -1]) cylinder(h = m + 2, d = d_hole);
}
