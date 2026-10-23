// Griffe: Literale, Variable, Raster, Drehung, berechnete Werte (Vitest-Fixture)
p = [20, 0, 10];
grid g {
    x: links = 0, rechts = 40;
    z: boden = 0, deckel = 20;
}
translate(p) cube([10, 10, 10]);
rotate([0, 0, 30]) cube([20, 4, 4]);
cylinder(h = 12, r = 4);
translate([1, 0, 0] * 2) sphere(3);
scale(2) cube(5);
t = 3;
hohl = true;
