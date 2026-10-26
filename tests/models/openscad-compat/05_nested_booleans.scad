// Zweck: nested booleans – cube intersected with a sphere, then a cross bore subtracted.
difference() {
    intersection() {
        cube(20, center = true);
        sphere(13);
    }
    union() {
        cylinder(h = 30, r = 4, center = true);
        rotate([90, 0, 0]) cylinder(h = 30, r = 4, center = true);
        rotate([0, 90, 0]) cylinder(h = 30, r = 4, center = true);
    }
}
translate([30, 0, -10]) difference() {
    cube([20, 20, 10]);
    translate([5, 5, 5]) cube([10, 10, 10]);
    union() {
        translate([10, 10, -1]) cylinder(h = 12, r = 2);
    }
}
