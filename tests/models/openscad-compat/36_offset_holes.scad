// Zweck: offset an Profilen mit Löchern – Löcher schrumpfen/wachsen, schließen sich, schmale Teile verschwinden.
module lochplatte() difference() {
    square([30, 20]);
    translate([8, 10]) circle(r = 4);
    translate([18, 6]) square([8, 8]);
}

linear_extrude(2) lochplatte();
translate([40, 0, 0]) linear_extrude(2) offset(r = 1) lochplatte();    // Löcher kleiner
translate([80, 0, 0]) linear_extrude(2) offset(r = -1) lochplatte();   // Löcher größer
translate([0, 30, 0]) linear_extrude(2) offset(r = 4.5) lochplatte();  // Löcher geschlossen
translate([40, 30, 0]) linear_extrude(2) offset(delta = 1) lochplatte();
// zwei Teile wachsen zusammen; ein schmaler Steg verschwindet nach innen
translate([0, 70, 0]) linear_extrude(2) offset(r = 1.5) {
    square([10, 10]);
    translate([12, 0]) square([10, 10]);
}
translate([40, 70, 0]) linear_extrude(2) offset(r = -1) union() {
    square([10, 10]);
    translate([10, 4]) square([15, 1.5]);
    translate([25, 0]) square([10, 10]);
}
