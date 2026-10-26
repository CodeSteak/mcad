// Purpose: cable clip – ring with opening, screw tab; intersection cuts the opening.
d_cable = 8; wall = 2; w = 8;
r_o = d_cable / 2 + wall;
difference() {
    union() {
        cylinder(h = w, r = r_o);
        translate([-r_o - 8, -r_o, 0]) cube([8, wall, w]);
    }
    translate([0, 0, -1]) cylinder(h = w + 2, d = d_cable);
    // Opening on top: wedge from the intersection of a cube and a rotated cube
    intersection() {
        translate([-3, 0, -1]) cube([6, r_o + 1, w + 2]);
        translate([0, r_o / 2, -1]) rotate([0, 0, 45]) cube([r_o, r_o, w + 2]);
    }
    translate([-r_o - 4, -r_o - 1, w / 2]) rotate([-90, 0, 0]) cylinder(h = wall + 2, d = 3);
}
