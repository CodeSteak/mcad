// Zweck: Skizzen mit Bögen (Plan 17.1) – ein Halter als Skizze aus Geraden und Bögen mit
// Raster in der Skizze, einer Bohrung (Gegenlauf, Windungsregel) und einem Langloch
// (`hole { … }`), extrudiert; daneben ein Drehknopf aus einer Skizze mit tangentialem Bogen
// und kubischer Bézier-Kurve, um die z-Achse gedreht.
module halter(l = 60, b = 24, r_loch = 4) sketch {
    grid s {
        x: links = 0, bogen = l - b / 2;
        y: unten = 0, mitte = b / 2, oben = b;
    }
    start(s[links, unten]);
    line(to = s[bogen, unten]);
    arc(to = s[bogen, oben], r = b / 2);        // Halbkreis am Ende
    line(to = s[links, oben]);
    close();
    // Bohrung um den Bogenmittelpunkt, im Uhrzeigersinn: ein Loch (Windungsregel)
    start([l - b / 2 + r_loch, b / 2]);
    arc(to = [l - b / 2 - r_loch, b / 2], r = r_loch, cw = true);
    arc(to = [l - b / 2 + r_loch, b / 2], r = r_loch, cw = true);
    // Langloch: ein Loch in jeder Laufrichtung
    hole {
        start([10, b / 2 - 3]);
        line(to = [26, b / 2 - 3]);
        arc(to = [26, b / 2 + 3], r = 3);
        line(to = [10, b / 2 + 3]);
        arc(to = [10, b / 2 - 3], r = 3);
    }
}

linear_extrude(5) halter();

translate([90, 12, 0]) rotate_extrude() sketch {
    start([0, 0]);
    line(to = [14, 0]);
    line(to = [14, 8]);
    arc(to = [12, 10]);                         // tangential an die Gerade
    bezier(c1 = [6, 10], c2 = [5, 16], to = [0, 16]);
    close();                                    // entlang der Achse zurück
}
