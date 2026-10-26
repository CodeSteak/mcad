// Purpose: cylinder with h/r, d, center and positional arguments.
cylinder(h = 10, r = 5);
translate([15, 0, 0]) cylinder(h = 8, d = 6);
translate([30, 0, 0]) cylinder(12, 3, 3);
translate([0, 15, 0]) cylinder(h = 10, r = 4, center = true);
translate([15, 15, 0]) cylinder(r = 2, h = 20, center = true);
