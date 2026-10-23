use <lib.scad>
module platte() { color("white") cube([30, 10, 2]); translate([5, 5, 2]) knopf(); translate([15, 5, 2]) knopf("teal"); translate([25,5,2]) boese(); }
module deko() translate([0, 20, 0]) knopf("teal");
teil: platte();
deko: deko();
translate([0, -20, 0]) color("yellow") cube(5);
