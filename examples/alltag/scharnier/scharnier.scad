// Zweck: Druckbares Scharnier 60 mm aus drei Bauteilen (Blatt A, Blatt B, Stift) – Maße per
// include, Teile per use; die Augen beider Blätter aus derselben Funktion (axiales Spiel),
// Freiräume per for, Blatt B um die Achse auf den Öffnungswinkel gedreht. Prüfen:
// `cad measure scharnier.scad --interference --clearance a b` (Spiel 0,4 mm) und Stift ↔ B.
// Maße in mm.
include <masse.scad>
use <teile.scad>

winkel = 180;     // Öffnungswinkel zwischen den Blättern (180 = flach; unter ca. 40° berühren sich die Blätter)

a: blatt(seite = 0, l = laenge, n = augen, r = auge_r, t = blatt_t, spiel = spiel, bohrung = bohrung_a);
rotate([180 - winkel, 0, 0])
    b: blatt(seite = 1, l = laenge, n = augen, r = auge_r, t = blatt_t, spiel = spiel, bohrung = bohrung_b);
translate(a.achse_anfang) stift: stift(l = laenge, d = stift_d);
