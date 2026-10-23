linear_extrude(5) color("red") square(10);
translate([15,0,0]) color("blue") linear_extrude(5) square(10);
translate([30,0,0]) linear_extrude(5) { color("red") square(5); color("blue") translate([5,0]) square(5); }
