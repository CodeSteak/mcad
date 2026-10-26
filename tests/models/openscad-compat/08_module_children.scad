// Zweck: modules with parameters and children() – feet at the corners of a plate.
module corners(w, d) {
    for (x = [0, w], y = [0, d]) translate([x, y, 0]) children();
}
module plate(w = 40, d = 30, t = 2) {
    cube([w, d, t]);
    translate([0, 0, -6]) corners(w - 6, d - 6) translate([3, 3, 0]) children(0);
}
module foot(r = 2) cylinder(h = 6, r = r);
plate(w = 50) foot(r = 2.5);
translate([0, 40, 0]) plate(d = 20, t = 3) foot();
