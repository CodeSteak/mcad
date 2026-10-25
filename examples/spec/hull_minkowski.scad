// Zweck: hull() und minkowski() in ihren exakten Sonderfällen (Plan 21): Platte mit runden
// Ecken und Langlöchern (2D-Hülle), abgerundeter Block (minkowski mit Kugel), Griff als Kapsel
// (Kugeln auf einer Achse), Lasche aus Quader und Zylinder gleicher Höhe (Prisma), Kissen aus
// gleich großen Kugeln (Hülle der Mittelpunkte + Offset) und eine Leiste aus linear_extrude +
// cylinder (Minkowski-Summe zweier Prismen).
module langloch(l = 20, d = 6) {
    hull() {
        circle(d = d);
        translate([l, 0]) circle(d = d);
    }
}

module platte(w = 60, t = 30, h = 4, r = 5) {
    difference() {
        linear_extrude(h) hull() {
            for (x = [0, w], y = [0, t]) translate([x, y]) circle(r);
        }
        for (y = [t / 4, 3 * t / 4]) translate([w / 4, y, -1]) linear_extrude(h + 2) langloch(w / 2, 5);
    }
}

platte();
translate([0, 50, 0]) minkowski() {
    cube([20, 20, 10]);
    sphere(2);
}
translate([40, 52, 0]) hull() {
    translate([0, 0, 3]) sphere(3);
    translate([0, 0, 25]) sphere(3);
}
translate([60, 52, 0]) hull() {
    translate([-5, -10, 0]) cube([5, 20, 4]);
    translate([12, 0, 0]) cylinder(r = 6, h = 4);
}
translate([0, 90, 2]) hull() {
    for (x = [0, 30], y = [0, 15], z = [0, 4]) translate([x, y, z]) sphere(2);
}
translate([50, 90, 0]) minkowski() {
    linear_extrude(3) polygon([[0, 0], [20, 0], [20, 4], [4, 4], [4, 15], [0, 15]]);
    cylinder(r = 1, h = 1);
}
