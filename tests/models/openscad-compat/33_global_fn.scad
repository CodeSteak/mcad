// Zweck: globales $fn = 64 nur für glatte Darstellung – bei uns exakte Geometrie mit Hinweis.
$fn = 64;
difference() {
    cylinder(h = 10, r = 8);
    translate([0, 0, 2]) cylinder(h = 10, r = 6);
}
translate([20, 0, 5]) sphere(5);
