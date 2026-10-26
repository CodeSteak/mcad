// Purpose: pipe bend via rotate_extrude(angle = 90) with two straight connectors.
r_bend = 20;
d_out = 8;
wall = 1.5;
module section() difference() {
    circle(d = d_out);
    circle(d = d_out - 2 * wall);
}
rotate_extrude(angle = 90) translate([r_bend, 0]) section();
// Connector at the start of the bend (plane y = 0), towards -y
translate([r_bend, 0, 0]) rotate([90, 0, 0]) linear_extrude(height = 10) section();
// Connector at the end of the bend (plane x = 0), towards -x
translate([0, r_bend, 0]) rotate([0, -90, 0]) linear_extrude(height = 10) section();
