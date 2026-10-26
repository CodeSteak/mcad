// Zweck: electronics box with lid and four screw bosses; lid lies next to it, ready to print.
w = 60; d = 40; h = 25; t = 2;
dome_d = 6; screw_d = 2.5; inset = 4.5;   // Bosses overlap the wall (no touching point)

module domes(hh) {
    for (x = [inset, w - inset], y = [inset, d - inset])
        translate([x, y, 0]) difference() {
            cylinder(h = hh, d = dome_d);
            translate([0, 0, 2]) cylinder(h = hh, d = screw_d);
        }
}

module box() {
    difference() {
        cube([w, d, h]);
        translate([t, t, t]) cube([w - 2 * t, d - 2 * t, h]);
        // Cable entry
        translate([-1, d / 2, h / 2]) rotate([0, 90, 0]) cylinder(h = t + 2, d = 6);
    }
    translate([0, 0, t]) domes(h - t - 1);
}

module lid() {
    difference() {
        cube([w, d, t]);
        for (x = [inset, w - inset], y = [inset, d - inset])
            translate([x, y, -1]) cylinder(h = t + 2, d = screw_d + 0.4);
    }
    // Centring rim
    translate([t + 0.3, t + 0.3, t]) difference() {
        cube([w - 2 * t - 0.6, d - 2 * t - 0.6, 2]);
        translate([1, 1, -1]) cube([w - 2 * t - 2.6, d - 2 * t - 2.6, 4]);
    }
}

echo(inner = [w - 2 * t, d - 2 * t, h - t]);
box();
translate([0, d + 10, 0]) lid();
