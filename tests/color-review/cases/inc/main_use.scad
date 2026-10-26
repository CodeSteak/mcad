use <lib.scad>
module plate() { color("white") cube([30, 10, 2]); translate([5, 5, 2]) knob(); translate([15, 5, 2]) knob("teal"); translate([25,5,2]) odd(); }
module deco() translate([0, 20, 0]) knob("teal");
part: plate();
deco: deco();
translate([0, -20, 0]) color("yellow") cube(5);
