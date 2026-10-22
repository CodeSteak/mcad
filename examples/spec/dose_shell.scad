// Zweck: shell(t) { körper; zone } – Dose aus einem verrundeten Vollkörper ($fillet),
// oben geöffnet über die Zone an der Rasterebene deckel; Anker für den Deckel.
module dose(w = 50, d = 35, h = 30, wand = 1.5) {
    grid g {
        x: links = 0, rechts = w;
        y: vorne = 0, hinten = d;
        z: boden = 0, deckel = h;
    }
    shell(wand) {
        box(g[links, vorne, boden], g[rechts, hinten, deckel], $fillet = 4);
        zone(g.deckel, d = 1);
    }
    anchor rand = g[links, vorne, deckel], dir = [0, 0, 1];
}

module deckel(w = 50, d = 35) {
    cube([w, d, 2], $fillet = 0.8);
}

k: dose();
translate(k.rand + [0, 0, 5]) rotate(to = dir(k.rand)) deckel();
