// Zweck: STEP-Export mit getrennten Körpern – Bauteile und Objekte, die sich nur berühren.
module block(w = 10) cube([w, 10, 10]);

a: block();
translate([10, 0, 0]) b: block();
translate([0, 10, 0]) color("red") cube([20, 5, 5]);
translate([0, 15, 0]) cube([20, 5, 5]);
