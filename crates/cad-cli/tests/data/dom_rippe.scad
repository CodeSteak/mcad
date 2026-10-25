// Zweck: Kernel-Fehler mit Ort – ein Schlüsselloch-Dom berührt eine Rippe tangential (die
// Vereinigung ist kein geschlossener Körper); die Meldung nennt beide Zeilen und die Stelle.
module dom(h = 10) {
    difference() {
        cylinder(h = h, r = 4);
        translate([0, 0, 2]) linear_extrude(h) union() {
            circle(2);
            slot([0, 0], [0, -2.5], 1);
        }
    }
}
module rippe(l = 20, h = 8) cube([2, l, h]);

cube([30, 30, 2]);
translate([11, 15, 0]) dom();
translate([5, 5, 0]) rippe();
