// Fixture: profile operations, surfaces and shell (web/src/logic/display.test.ts).
linear_extrude(10, center = true) square([20, 10]);
translate([40, 0, 0]) rotate_extrude(angle = 270) translate([10, 0]) square([3, 12]);
translate([0, 40, 0]) loft() {
    circle(8);
    translate([0, 0, 20]) square(10, center = true);
}
translate([40, 40, 0]) sweep(path = [[0, 0, 0], [20, 0, 10], [40, 10, 10]], interp = "linear") circle(2);
translate([0, 80, 0]) thicken(1.5) surface(points = [for (i = [0:3]) [for (j = [0:3]) [i * 10, j * 10, (i - 1.5) * (j - 1.5)]]]);
grid g { z: oben = 15; }
translate([50, 80, 0]) shell(1.2) {
    box([0, 0, 0], [30, 20, 15]);
    zone(g.oben, d = 1);
}
