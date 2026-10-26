// Zweck: crown-gear-like ring – teeth evenly spaced via for and rotate.
teeth = 16;
r_in = 12;
r_out = 16;
h = 5;
difference() {
    cylinder(h = h, r = r_out);
    translate([0, 0, -1]) cylinder(h = h + 2, r = r_in);
}
for (i = [0 : teeth - 1])
    rotate([0, 0, i * 360 / teeth])
        translate([r_out - 0.5, -1, 0]) cube([3, 2, h]);
