// Zweck: linear_extrude with twist and scale – twisted bar and tapered base.
linear_extrude(height = 20, twist = 90) square(8, center = true);
translate([25, 0, 0]) linear_extrude(height = 10, scale = 0.5) square(12, center = true);
translate([0, 25, 0])
    linear_extrude(height = 12, twist = -60, scale = [1, 0.5], center = true) square([10, 6], center = true);
