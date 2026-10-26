// Purpose: offset on profiles with holes – holes shrink/grow, close, thin parts vanish.
module hole_plate() difference() {
    square([30, 20]);
    translate([8, 10]) circle(r = 4);
    translate([18, 6]) square([8, 8]);
}

linear_extrude(2) hole_plate();
translate([40, 0, 0]) linear_extrude(2) offset(r = 1) hole_plate();    // Holes smaller
translate([80, 0, 0]) linear_extrude(2) offset(r = -1) hole_plate();   // Holes larger
translate([0, 30, 0]) linear_extrude(2) offset(r = 4.5) hole_plate();  // Holes closed
translate([40, 30, 0]) linear_extrude(2) offset(delta = 1) hole_plate();
// two parts grow together; a thin web vanishes inwards
translate([0, 70, 0]) linear_extrude(2) offset(r = 1.5) {
    square([10, 10]);
    translate([12, 0]) square([10, 10]);
}
translate([40, 70, 0]) linear_extrude(2) offset(r = -1) union() {
    square([10, 10]);
    translate([10, 4]) square([15, 1.5]);
    translate([25, 0]) square([10, 10]);
}
