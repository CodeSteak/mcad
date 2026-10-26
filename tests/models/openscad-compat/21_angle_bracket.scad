// Purpose: angle bracket (L holder) with stiffening rib and slots.
w = 30; l = 40; t = 4;
// Slot without hull(): two circles and a rectangle
module slot(len, d) {
    translate([-len / 2, 0]) circle(d = d);
    translate([len / 2, 0]) circle(d = d);
    square([len, d], center = true);
}
difference() {
    union() {
        cube([l, w, t]);
        cube([t, w, l]);
        // Rib
        translate([0, w / 2 - t / 2, 0]) rotate([90, 0, 0]) mirror([0, 0, 1])
            linear_extrude(height = t) polygon([[t, t], [l - 5, t], [t, l - 5]]);
    }
    translate([l - 12, w / 4, -1]) linear_extrude(t + 2) slot(8, 5);
    translate([l - 12, 3 * w / 4, -1]) linear_extrude(t + 2) slot(8, 5);
    translate([-1, w / 4, l - 12]) rotate([0, 90, 0]) cylinder(h = t + 2, d = 5);
    translate([-1, 3 * w / 4, l - 12]) rotate([0, 90, 0]) cylinder(h = t + 2, d = 5);
}
