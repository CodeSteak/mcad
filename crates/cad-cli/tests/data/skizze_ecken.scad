// Skizzen Schritt 2: corner, fillet2d, offset, stroke
linear_extrude(2) sketch {
    start([0, 0]);
    line(to = [30, 0]);
    corner(r = 4);
    line(to = [30, 20]);
    corner(r = 4);
    line(to = [0, 20]);
    corner(chamfer = 3);
    close();
}
translate([40, 0, 0]) linear_extrude(2) fillet2d(3) square([30, 20]);
translate([0, 30, 0]) linear_extrude(2) offset(r = 2) square([20, 10]);
translate([40, 30, 0]) linear_extrude(2) stroke(2, cap = "flat") sketch(closed = false) {
    start([10, 0]);
    arc(center = [0, 0], angle = 90);
}
