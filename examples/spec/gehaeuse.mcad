// Zweck: Spec-Beispiel „gehaeuse“ – Raster g im Modul, Box zwischen Rasterpunkten, Anker
// nach außen, benannte Instanz k, Deckel am Anker und ein zweites Raster aus k.ecke.x.
module gehaeuse(w = 40, d = 30, h = 20, t = 3) {
    grid g {
        x: links = 0, innen_l = t, innen_r = w - t, rechts = w;
        y: vorne = 0, hinten = d;
        z: boden = 0, deckel = h;
    }

    difference() {
        box(g[links, vorne, boden], g[rechts, hinten, deckel]);
        box(g[innen_l, vorne, t], g[innen_r, hinten, deckel]);
    }

    anchor ecke = g[rechts, hinten, deckel];
    anchor oben = g[links, vorne, deckel], dir = [0, 0, 1];
}

module deckel() cube([10, 10, 2]);

k: gehaeuse(w = 60);
translate(k.ecke) deckel();
translate(k.oben) rotate(to = dir(k.oben)) deckel();

grid h {
    x: rand = k.ecke.x;
}
