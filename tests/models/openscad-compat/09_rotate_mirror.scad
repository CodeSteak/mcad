// Purpose: rotate in all forms and mirror – star-shaped bracket.
module arm() translate([5, 0, 0]) cube([20, 4, 4]);
arm();
mirror([1, 0, 0]) arm();
rotate([0, 0, 90]) arm();
rotate(a = 45, v = [0, 0, 1]) translate([0, 0, 10]) arm();
rotate(30) translate([0, 0, 20]) cube([10, 2, 2]);
rotate([90, 0, 0]) translate([0, 0, -2]) cylinder(h = 4, r = 3);
mirror([0, 1, 1]) translate([0, 10, 0]) cube(3);
