// Zweck: Schild mit Relief-Text (Plan 17.4, Bericht 16) – Grundplatte mit Rand, erhabener Text in
// zwei Schriften – mehrfarbig (3MF mit einem Filament je Farbe).
// Schriften: mitgeliefert sind JetBrains Mono (Standard), Inter und Allerta Stencil, je mit
// Stil (`"Inter:style=Bold"`); eigene Schriften mit `font = "datei.ttf"` (Datei im Projekt).
// `size` wie in OpenSCAD: Versalhöhe etwa `size`.

breite = 100;
hoehe = 44;
dicke = 3;
r = 5;          // Eckenradius
rand = 2;       // Breite des Rands
relief = 1.2;   // Höhe von Text und Rand

module platte(b, h, r) sketch {
    start([r, 0]);
    line(to = [b - r, 0]);
    arc(center = [b - r, r], angle = 90);
    line(to = [b, h - r]);
    arc(center = [b - r, h - r], angle = 90);
    line(to = [r, h]);
    arc(center = [r, h - r], angle = 90);
    line(to = [0, r]);
    arc(center = [r, r], angle = 90);
}

color("white") linear_extrude(dicke) platte(breite, hoehe, r);

// Rand
color("blue") translate([0, 0, dicke]) linear_extrude(relief) difference() {
    platte(breite, hoehe, r);
    translate([rand, rand]) platte(breite - 2 * rand, hoehe - 2 * rand, r - rand);
}

// Titel und Untertitel erhaben
color("red") translate([breite / 2, 26, dicke])
    linear_extrude(relief) text("Werkstatt", size = 11, font = "Inter:style=Bold", halign = "center", valign = "center");
color("black") translate([breite / 2, 9, dicke])
    linear_extrude(relief) text("ZUTRITT NUR MIT BRILLE", size = 4.2, font = "Allerta Stencil", halign = "center");
