// Zweck: Handyständer – geneigte Lehne über rotate, Lippe, Kabelschlitz; Profil per polygon.
angle = 65; depth = 60; width = 70; t = 5;
lean = 80;
module profile() polygon([
    [0, 0], [depth, 0], [depth, t], [t + 10, t], [t + 10, t + 8], [t + 6, t + 8], [t + 6, t],
    [t, t], [t + lean * cos(angle) - t, lean * sin(angle)], [lean * cos(angle) - t, lean * sin(angle)]
]);
difference() {
    rotate([90, 0, 0]) translate([0, 0, -width]) linear_extrude(height = width) profile();
    // Kabelschlitz
    translate([-1, width / 2 - 6, -1]) cube([t + 12, 12, t + 2]);
}
