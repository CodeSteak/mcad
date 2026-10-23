module wrap() { color("green") children(); }
module bar(c) color(c) cube([10, 2, 2]);
wrap() cube(4);
for (i = [0:2]) translate([0, 5 + i * 3, 0]) bar(["red", "blue", "red"][i]);
wrap() translate([12,0,0]) color("yellow") cube(3);
