// Zweck: drei Bauteile für `cad measure`: a und b überschneiden sich absichtlich
// (2 × 10 × 10 = 200 mm³), c steht 5 mm neben b.
module klotz(s = 10) cube(s);

a: klotz();
translate([8, 0, 0]) b: klotz();
translate([23, 0, 0]) c: klotz();
