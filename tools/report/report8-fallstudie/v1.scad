// Case study of report 8: wall bracket for a rod, developed with the CLI only.
// Base plate on the wall (y = 0), arm with a bore for the rod, two screw holes.
module halter(b = 40, t = 5, h = 50, a = 30, d = 20, spiel = 0) {
    grid g {
        x: links = 0, loch_l = 10, mitte = b / 2, loch_r = b - 10, rechts = b;
        y: wand = 0, platte = t, achse = t + a, vorn = t + a + d / 2 + t;
        z: unten = 0, loch = 10, arm = h - d - 2 * t, achse_z = h - t - d / 2, oben = h;
    }
    difference() {
        union() {
            box(g[links, wand, unten], g[rechts, platte, oben]);
            box(g[links, platte, arm], g[rechts, vorn, oben]);
        }
        cylinder(g[-1, achse, achse_z], g[b + 1, achse, achse_z], r = (d + spiel) / 2);
        for (x = [g.loch_l, g.loch_r])
            cylinder(g[x, -1, loch], g[x, t + 1, loch], r = 2.25);
    }
    anchor stange = g[links, achse, achse_z], dir = [1, 0, 0];
    anchor schraube = g[loch_l, wand, loch], dir = [0, -1, 0];
}

module stange(l = 60, d = 20) cylinder(h = l, r = d / 2);

halter: halter();
translate(halter.stange) rotate(to = dir(halter.stange)) translate([0, 0, -10]) stange: stange();
