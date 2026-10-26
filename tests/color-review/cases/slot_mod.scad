module bar(c) color(c) cube([10, 2, 2]);
bar("red");
translate([0, 5, 0]) bar("blue");
translate([0, 10, 0]) bar("green");
