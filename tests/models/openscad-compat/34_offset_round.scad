// Purpose: offset(r) – rounding outwards and inwards at convex and concave corners, circle, polygon.
module l_shape() polygon([[0, 0], [20, 0], [20, 8], [8, 8], [8, 20], [0, 20]]);

linear_extrude(2) offset(r = 2) square(10);                     // outwards: corners become arcs
translate([20, 0, 0]) linear_extrude(2) offset(r = -2) square(10);   // inwards: corners stay sharp
translate([40, 0, 0]) linear_extrude(2) offset(3) circle(r = 5);     // positional = r
translate([60, 0, 0]) linear_extrude(2) offset(r = 1.5) l_shape();   // concave corner stays sharp
translate([90, 0, 0]) linear_extrude(2) offset(r = -1.5) l_shape();  // concave corner becomes round
// Opening (first inwards, then outwards): sharp corners become round, thin webs vanish
translate([0, 40, 0]) linear_extrude(3) offset(r = 2) offset(r = -2) union() {
    square([30, 12]);
    translate([12, 0]) square([3, 30]);
    translate([0, 20]) square([1.5, 10]);
}
