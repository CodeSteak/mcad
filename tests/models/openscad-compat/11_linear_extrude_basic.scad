// Purpose: linear_extrude with height as name and positional, center = true.
linear_extrude(height = 5) square([20, 10]);
translate([30, 0, 0]) linear_extrude(8, center = true) square(6, center = true);
translate([0, 20, 0]) linear_extrude(height = 3) polygon([[0, 0], [15, 0], [0, 10]]);
