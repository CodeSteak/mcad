// Zweck: Modifier – # markiert (bleibt im Ergebnis), % ist Hilfsgeometrie, * ist ausgeblendet.
difference() {
    cube([20, 20, 10]);
    # translate([10, 10, -1]) cylinder(h = 12, r = 4);
}
% translate([0, 0, 10]) cube([20, 20, 1]);      // Deckel nur zur Anschauung
* translate([30, 0, 0]) sphere(10);             // abgeschaltet
translate([30, 0, 0]) cube(5);
