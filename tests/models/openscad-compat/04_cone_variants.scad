// Purpose: cones and truncated cones via r1/r2, d1/d2 and apex (d2 = 0).
cylinder(h = 10, r1 = 6, r2 = 3);
translate([15, 0, 0]) cylinder(h = 10, d1 = 10, d2 = 0);
translate([30, 0, 0]) cylinder(h = 6, r1 = 2, r2 = 5, center = true);
translate([0, 15, 0]) cylinder(8, 4, 1);
