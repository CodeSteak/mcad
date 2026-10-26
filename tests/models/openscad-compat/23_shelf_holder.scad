// Zweck: shelf bracket – profile via polygon, linear_extrude, screw holes with countersink (cone).
l = 50; h = 40; t = 5; b = 20;
module countersink() {
    cylinder(h = t + 2, d = 4);
    translate([0, 0, t - 2]) cylinder(h = 2.01, d1 = 4, d2 = 8);
}
difference() {
    rotate([90, 0, 0]) translate([0, 0, -b]) linear_extrude(height = b)
        polygon([[0, 0], [l, 0], [l, t], [t + 8, t], [t, h - 8], [t, h], [0, h]]);
    // Holes in the wall side
    for (z = [12, h - 8]) translate([-1, b / 2, z]) rotate([0, 90, 0]) cylinder(h = t + 2, d = 4);
    // Countersunk hole in the support
    translate([l - 10, b / 2, -1]) countersink();
}
