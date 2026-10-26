// Zweck: Offene Skizzen als `sweep`-Pfad (Plan 17.1): ein Rohrbogen aus Gerade, tangentialem
// Bogen und Gerade (glatter Pfad: ein Kurvenzug, das Profil ist ein Kreisring) in der xz-Ebene,
// und ein Rahmen aus einem Rechteckprofil entlang eines Pfads mit Ecken (Gehrung).
sweep() {
    rotate([90, 0, 0]) sketch(closed = false) {
        start([0, 0]);
        line(to = [30, 0]);
        arc(to = [45, 15]);                     // tangential: Radius 15
        line(by = [0, 25]);
    }
    difference() {
        circle(4);
        circle(3);
    }
}

translate([0, 20, 0]) sweep() {
    sketch(closed = false) {
        start([0, 0]);
        line(to = [40, 0]);
        line(to = [40, 30]);
        line(to = [0, 30]);
    }
    square([3, 6], center = true);
}
