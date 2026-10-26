// Zweck: scale (Zahl und Vektor) und multmatrix (Scherung, 3x4 und 4x4).
scale(2) cube(5);
translate([20, 0, 0]) scale([1, 2, 0.5]) cube(10);
translate([40, 0, 0]) multmatrix([[1, 0, 0.5, 0],
                                  [0, 1, 0, 0],
                                  [0, 0, 1, 0]]) cube(10);
translate([0, 30, 0]) multmatrix([[1, 0.3, 0, 5],
                                  [0, 1, 0, 0],
                                  [0, 0, 1, 2],
                                  [0, 0, 0, 1]]) cube([10, 10, 4]);
