// Zweck: Lüftungsgitter – Rahmen per linear_extrude eines Rechtecks mit Ausschnitt,
// Schlitze an indizierten Rasterebenen (linear_extrude in difference), Fase an den neuen
// Kanten (difference(chamfer = …)) und Befestigungslaschen.
module gitter(w = 80, d = 50, t = 3, n = 7, schlitz = 4) {
    grid g {
        x: links = 0, rechts = w, s = [for (i = [0 : n - 1]) 10 + i * (w - 20) / (n - 1)];
        y: vorne = 0, hinten = d, innen_v = 8, innen_h = d - 8;
        z: boden = 0, oben = t;
    }

    difference(chamfer = 0.6) {
        linear_extrude(t) square([w, d]);
        for (i = [0 : n - 1])
            translate(g[s[i], innen_v, -1])
                linear_extrude(t + 2) translate([-schlitz / 2, 0]) square([schlitz, d - 16]);
    }

    // Laschen links und rechts
    for (x = [-8, w])
        translate([x, d / 2 - 6, 0]) linear_extrude(t / 2) square([8, 12]);

    anchor mitte = g[links, vorne, oben] + [w / 2, d / 2, 0], dir = [0, 0, 1];
}

gitter();
