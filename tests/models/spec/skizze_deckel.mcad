// Zweck: Gehäusedeckel (Plan 17.3) – der Umriss ist eine Skizze mit verrundeten Ecken
// (`corner`), der Steckrand ein Ring aus zwei Offsets desselben Umrisses nach innen
// (`offset(r = -…)`: die Eckradien werden kleiner, bleiben aber exakte Bögen), dazu vier
// Schraubenlöcher in den Ecken.
l = 80;       // Außenmaß
b = 50;
r = 6;        // Eckradius außen
wand = 2;     // Wandstärke des Gehäuses
spiel = 0.2;  // Luft zwischen Steckrand und Wand
steg = 1.5;   // Breite des Steckrands

module umriss() sketch {
    start([0, 0]);
    corner(r = r);
    line(to = [l, 0]);
    corner(r = r);
    line(to = [l, b]);
    corner(r = r);
    line(to = [0, b]);
    corner(r = r);
    close();
}

difference() {
    linear_extrude(2) umriss();
    for (p = [[r, r], [l - r, r], [l - r, b - r], [r, b - r]])
        translate([p[0], p[1], -1]) cylinder(h = 4, d = 2.5);
}
translate([0, 0, 2]) linear_extrude(5) difference() {
    offset(r = -(wand + spiel)) umriss();
    offset(r = -(wand + spiel + steg)) umriss();
}
