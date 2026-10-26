// Zweck: cube in allen Formen – Zahl, Vektor, center = true/false, Positionsargumente.
cube(10);
translate([15, 0, 0]) cube([20, 10, 5]);
translate([50, 5, 5]) cube([8, 10, 10], center = true);
translate([0, 20, 0]) cube(size = [4, 4, 12], center = false);
translate([20, 25, 3]) cube(6, true);
