// Zweck: Rohrbogen per rotate_extrude(angle = 90) mit zwei geraden Anschlussstücken.
r_bend = 20;
d_out = 8;
wall = 1.5;
module section() difference() {
    circle(d = d_out);
    circle(d = d_out - 2 * wall);
}
rotate_extrude(angle = 90) translate([r_bend, 0]) section();
// Anschluss am Bogenanfang (Ebene y = 0), nach -y
translate([r_bend, 0, 0]) rotate([90, 0, 0]) linear_extrude(height = 10) section();
// Anschluss am Bogenende (Ebene x = 0), nach -x
translate([0, r_bend, 0]) rotate([0, -90, 0]) linear_extrude(height = 10) section();
