// Zweck: Kabelkanal entlang eines Pfads (Plan 17.3) – der Pfad ist eine offene Skizze aus
// Geraden, einem tangentialen Bogen und einer verrundeten Ecke (`corner`); `stroke` macht
// daraus den Boden (volle Breite, flache Enden) und die Wände (Differenz mit einem schmaleren
// Strich, dessen eckige Enden über den Kanal hinausreichen: die Enden bleiben offen); dazu
// Rippen im Boden als Zickzack mit Gehrung.
breite = 16;   // außen
wand = 2;
hoehe = 12;
boden = 2;

module pfad() {
    sketch(closed = false) {
        start([0, 0]);
        line(to = [40, 0]);
        arc(to = [60, 20]);          // tangential: Viertelkreis r = 20
        line(to = [60, 50]);
        corner(r = 12);
        line(to = [20, 50]);
    }
}

linear_extrude(boden) stroke(breite, cap = "flat") pfad();
linear_extrude(hoehe) difference() {
    stroke(breite, cap = "flat") pfad();
    stroke(breite - 2 * wand, cap = "square") pfad();
}
// Rippen auf dem Boden des geraden Anfangsstücks
translate([0, 0, boden]) linear_extrude(1) stroke(1.2, cap = "round", join = "miter") sketch(closed = false) {
    start([4, -4]);
    for (i = [1 : 8]) line(to = [4 + 4 * i, i % 2 == 0 ? -4 : 4]);
}
