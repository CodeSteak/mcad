module m() { color("white") cube([20, 10, 2]); color("red") translate([2, 2, 1]) cube([5, 5, 2]); }
module r() resize([10, 10, 3]) m();
module mi() mirror([1, 0, 0]) rotate([0, 0, 30]) m();
a: r();
b: mi();
