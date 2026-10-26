color("red") cube(10);
%color("blue") translate([20,0,0]) cube(5);
#color("green") translate([0,20,0]) cube(6);
*color("yellow") translate([20,20,0]) cube(7);
difference() { color("teal") translate([40,0,0]) cube(8); #color("pink") translate([42,2,-1]) cube([4,4,10]); }
