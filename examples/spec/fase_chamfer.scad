// Zweck: Fasen in allen drei Formen – chamfer-Block mit Zone, union(chamfer = …) und
// $chamfer auf Primitiven; dazu $fillet für einen verrundeten Knopf.
grid g {
    x: a = 0, b = 40;
    y: v = 0, h = 25;
    z: u = 0, o = 10;
}

chamfer(1) {
    box(g[a, v, u], g[b, h, o]);
    zone(g.o, d = 1);
}

translate([0, 35, 0]) union(chamfer = 0.5) {
    cube([30, 10, 5]);
    translate([10, 0, 0]) cube([10, 10, 15]);
}

translate([50, 0, 0]) cube(10, $chamfer = 1);
translate([55, 30, 0]) cylinder(h = 8, r = 5, $fillet = 1);
