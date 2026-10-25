// Zweck: Platte mit Bohrung, Farbe, Modifier `#`, Variable mit Literal und echo.
r = 4;
module platte(w = 40, d = 30, t = 5) {
    difference() {
        color("orange") cube([w, d, t]);
        translate([w / 2, d / 2, -1]) cylinder(h = t + 2, r = r);
        #translate([5, 5, -1]) cylinder(h = t + 2, r = 2);
    }
}
platte();
echo("fertig", r);
