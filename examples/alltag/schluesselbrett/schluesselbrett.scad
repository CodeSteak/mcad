// Zweck: Schlüsselbrett mit fünf eingeklebten Haken (Baugruppe) – Brett mit über Zonen
// verrundeten Ecken und gefaster Vorderkante, Schlüssellochaufhängung hinten, Briefrille oben;
// Haken als sweep entlang eines Spline-Pfads mit Kugelknauf und Zapfen, als Bauteile h[0] …
// h[4] in einer Schleife (Export je Haken). Maße per include, Teile per use. Passung Zapfen ↔
// Bohrung: `cad measure schluesselbrett.scad --interference --clearance brett h[0]`. Maße in mm.
include <masse.scad>
use <teile.scad>

brett: brett();
for (i = [0 : anzahl - 1])
    translate([haken_x(i), 0, haken_z]) h[i]: haken();
