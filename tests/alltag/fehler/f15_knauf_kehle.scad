// F15: union(fillet = …) aus sweep (Kreis d 6) und Kugel d 9 am Pfadende scheitert für jeden
// Radius (auch 0,6); mit Kugel d 10 geht es. Fragil, ohne erkennbaren geometrischen Grund.
// ERWARTET: fehlerfrei
union(fillet = 1) {
    sweep(path = [[0, 0, 0], [0, -13, -1], [0, -22, 4], [0, -23, 10]]) circle(d = 6);
    translate([0, -23, 10]) sphere(d = 9);
}
