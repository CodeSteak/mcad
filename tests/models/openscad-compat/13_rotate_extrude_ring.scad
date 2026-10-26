// Zweck: rotate_extrude of a translated circle (ring) and of a rectangle (disc with hole).
rotate_extrude() translate([10, 0]) circle(r = 2);
translate([30, 0, 0]) rotate_extrude() translate([4, 0]) square([3, 5]);
