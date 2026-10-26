// Purpose: hex nut M5 – $fn = 6 at the call creates the hexagon, the bore is round.
s = 8;            // Width across flats
m = 4;            // Height
d_hole = 5;
e = s / cos(30);  // Width across corners
echo(across_corners = e);
difference() {
    cylinder(h = m, d = e, $fn = 6);
    translate([0, 0, -1]) cylinder(h = m + 2, d = d_hole);
}
// Washer next to it, square nut ($fn = 4)
translate([15, 0, 0]) difference() {
    cylinder(h = m, r = 5, $fn = 4);
    translate([0, 0, -1]) cylinder(h = m + 2, d = d_hole);
}
