// Purpose: offset(delta) – mitre at convex corners, with chamfer = true chamfers, negative inwards.
module star() polygon([for (i = [0 : 9]) let(a = i * 36, r = i % 2 == 0 ? 10 : 5) [r * cos(a), r * sin(a)]]);
module l_shape() polygon([[0, 0], [20, 0], [20, 8], [8, 8], [8, 20], [0, 20]]);

linear_extrude(2) offset(delta = 2) square(10);
translate([20, 0, 0]) linear_extrude(2) offset(delta = 2, chamfer = true) square(10);
translate([40, 0, 0]) linear_extrude(2) offset(delta = -2) square(10);
translate([65, 10, 0]) linear_extrude(2) offset(delta = 1) star();
translate([90, 10, 0]) linear_extrude(2) offset(delta = 1, chamfer = true) star();
translate([0, 30, 0]) linear_extrude(2) offset(delta = 1.5) l_shape();
translate([30, 30, 0]) linear_extrude(2) offset(delta = -1.5) l_shape();
translate([60, 30, 0]) linear_extrude(2) offset(delta = -1.5, chamfer = true) l_shape();
