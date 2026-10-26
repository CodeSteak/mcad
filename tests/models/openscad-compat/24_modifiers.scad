// Zweck: modifiers – # highlights (stays in the result), % is helper geometry, * is not shown.
difference() {
    cube([20, 20, 10]);
    # translate([10, 10, -1]) cylinder(h = 12, r = 4);
}
% translate([0, 0, 10]) cube([20, 20, 1]);      // Lid only for illustration
* translate([30, 0, 0]) sphere(10);             // switched off
translate([30, 0, 0]) cube(5);
