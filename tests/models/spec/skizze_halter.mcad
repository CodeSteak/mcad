// Zweck: Winkelhalter mit verrundetem Profil (Plan 17.3) – L-Profil als Polygon; `fillet2d`
// mit einer Zone rundet nur die Innenecke groß (Kerbwirkung), ein äußerer `fillet2d` alle
// übrigen Ecken klein (die Übergänge des großen Bogens sind tangential und zählen nicht);
// extrudiert, mit Langlöchern aus `stroke` einer Strecke (runde Enden).
t = 6;       // Wandstärke
a = 40;      // Schenkellänge
h = 30;      // Breite des Halters
r_innen = 5;

module profil() polygon([[0, 0], [a, 0], [a, t], [t, t], [t, a], [0, a]]);

module langloch(von, bis, d = 4.5) stroke(d) sketch(closed = false) {
    start(von);
    line(to = bis);
}

difference() {
    linear_extrude(h) fillet2d(1) {
        fillet2d(r_innen) {
            profil();
            translate([t, t]) circle(1);   // Zone: die Innenecke
        }
    }
    // Langlöcher im Boden (xz-Ebene, durch die Wand in y)
    translate([0, -1, 0]) rotate([-90, 0, 0]) linear_extrude(t + 2) langloch([16, -h / 2], [32, -h / 2]);
    // und in der Wand (yz-Ebene, durch die Wand in x)
    translate([-1, 0, 0]) rotate([90, 0, 90]) linear_extrude(t + 2) langloch([16, h / 2], [32, h / 2]);
}
