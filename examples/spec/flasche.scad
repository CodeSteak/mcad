// Zweck: Flasche – rotate_extrude eines Polygonprofils (Profil-x = Radius, Profil-y = Höhe;
// angezeigt in der xz-Ebene mit der Drehachse z), Teilumlauf als Schnittmodell daneben,
// Verrundung der Schulter- und Bodenkante über Zonen um die Rasterebenen.
module flasche(r = 22, h = 80, hals = 9, hh = 16, angle = 360) {
    grid g {
        x: achse = 0, innen = hals, aussen = r;
        y: boden = 0, schulter = h - hh - 10, hals_u = h - hh, oben = h;
    }

    fillet(3) {
        rotate_extrude(angle = angle)
            polygon([g[achse, boden, _], g[aussen, boden, _], g[aussen, schulter, _],
                     g[innen, hals_u, _], g[innen, oben, _], g[achse, oben, _]]);
        translate([0, 0, g.schulter]) cylinder(h = 2, r = r + 2, center = true); // Schulterkante
        translate([0, 0, g.boden]) cylinder(h = 1, r = r + 2, center = true);    // Bodenkante
    }
}

flasche();
translate([60, 0, 0]) flasche(angle = 270);
