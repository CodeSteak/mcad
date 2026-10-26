// Zweck: 2D booleans on circle/square ahead of linear_extrude – spacer discs in multiple sizes.
sizes = [[8, 3, 2], [10, 4, 3], [12, 5, 4], [16, 6.4, 5]];   // [outside, inside, h]
for (i = [0 : len(sizes) - 1]) {
    s = sizes[i];
    translate([i * 20, 0, 0]) linear_extrude(height = s[2]) difference() {
        circle(d = s[0]);
        circle(d = s[1]);
    }
}
// Spacer with a groove: rectangle minus circle, united with a web.
translate([0, 20, 0]) linear_extrude(height = 4) union() {
    difference() {
        square([30, 10]);
        translate([15, 10]) circle(r = 4);
    }
    translate([0, -3]) square([30, 3]);
}
