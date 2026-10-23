// Maße von Pflanztopf und Untersetzer (per include)
topf_unten = 72;        // Seitenlänge des abgerundeten Quadrats am Boden
topf_oben_d = 100;      // Durchmesser der runden Öffnung
topf_h = 90;            // Höhe ohne Füße
wand = 2.2;
boden = 3;
fuss_h = 4;             // Füße (Wasser läuft unter dem Topf ab)
spalt = 4;              // Abstand Topf ↔ Rand des Untersetzers

// Superellipse |x/a|^p + |y/a|^p = 1 als Stützpunkte für spline (p = 2: Kreis, groß: Quadrat)
function superellipse(a, p = 5, n = 24) =
    [for (i = [0 : n - 1]) let (w = i * 360 / n, c = cos(w), s = sin(w))
        [a * sign(c) * pow(abs(c), 2 / p), a * sign(s) * pow(abs(s), 2 / p)]];
