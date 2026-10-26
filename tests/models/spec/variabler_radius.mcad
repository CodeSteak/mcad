// Zweck: variabler Radius – fillet(r = function (p) …) { körper; zonen } und
// difference(fillet = function (p) …): der Radius ist eine Funktion der Position p
// (im System der fillet-/difference-Anweisung), ausgewertet entlang jeder Kante.
grid g {
    x: links = 0, rechts = 40;
    y: vorne = 0, hinten = 30;
    z: boden = 0, mitte = 10, deckel = 20;
}

// unter der Mitte Radius 2, darüber 4; die senkrechten Kanten gehen stetig von 2 auf 4
fillet(r = function (p) p.z < g.mitte ? 2 : 4) {
    box(g[links, vorne, boden], g[rechts, hinten, deckel]);
}

// Tasche mit verrundetem Rand, der Radius wächst entlang x (1,125 bis 1,875 am Rand)
translate([60, 0, 0]) difference(fillet = function (p) 1 + p.x / 40) {
    cube([40, 30, 10]);
    translate([5, 8, 4]) cube([30, 14, 10]);
}

// nur die vier Deckelkanten, Radius aus einer Tabelle (lookup) über x
translate([120, 0, 0]) fillet(r = function (p) lookup(p.x, [[0, 1.5], [20, 4], [40, 1.5]])) {
    box(g[links, vorne, boden], g[rechts, hinten, deckel]);
    zone(g.deckel, d = 1);
}
