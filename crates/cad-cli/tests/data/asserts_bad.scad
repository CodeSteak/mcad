cube([40, 30, 3]);
assert_keepout() translate([10, 10, 2]) cube([10, 10, 5]);
assert_touches() { translate([0, 40, 0]) cube(10); translate([10.2, 40, 0]) cube(10); }
