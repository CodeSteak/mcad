// F6 (Kernel-Schwäche): Fase/Verrundung der Kanten einer Ebene scheitert, wenn eine konvexe
// Kante an einer kurzen konkaven endet (Stufe vor einer hohen Wand): „chamfer failed:
// SM_ERR (1001)“; `cad render --line 5` zeigt die konkave Kante rot. Ebenso: Rippenfüße samt
// kurzer Außenkanten (Kühlkörper), Oberkanten des vorderen Fachs (Stiftebox).
// ERWARTET: fehlerfrei
chamfer(0.5) {
    difference() {
        union() {
            cube([40, 30, 20]);
            translate([0, 20, 0]) cube([40, 10, 50]);
        }
        translate([3, 3, 3]) cube([34, 17, 30]);
    }
    translate([20, 15, 20]) cube([60, 60, 0.5], center = true);
}
