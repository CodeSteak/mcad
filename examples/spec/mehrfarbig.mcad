// Zweck: Mehrfarbig drucken (Bericht 11) – Türschild mit Schrift-Relief und Spielfigur als
// zwei Bauteile, Farben aus der Palette, ein fester Filament-Slot; Export als 3MF mit einem
// Teil je Farbe für Bambu Studio / OrcaSlicer.

// Blockbuchstaben aus Balken: s = Strichstärke, b × h = Feld des Buchstabens.
module balken(x, y, w, h, t) translate([x, y, 0]) cube([w, h, t]);

module buchstabe(c, b = 8, h = 12, s = 2, t = 1.2) {
    if (c == "C") {
        balken(0, 0, s, h, t);
        balken(0, h - s, b, s, t);
        balken(0, 0, b, s, t);
    } else if (c == "A") {
        balken(0, 0, s, h, t);
        balken(b - s, 0, s, h, t);
        balken(0, h - s, b, s, t);
        balken(0, h / 2 - s / 2, b, s, t);
    } else if (c == "D") {
        // Balken überlappen (nur Kanten berühren gäbe keinen geschlossenen Körper)
        balken(0, 0, s, h, t);
        balken(0, h - s, b - s / 2, s, t);
        balken(0, 0, b - s / 2, s, t);
        balken(b - s, s / 2, s, h - s, t);
    }
}

module schild(w = 60, d = 28, t = 3, rand = 2.5) {
    grid g {
        x: links = 0, schrift = 15, rechts = w;
        y: vorne = 0, zeile = 8, hinten = d;
        z: boden = 0, oben = t;
    }
    difference() {
        union() {
            // Grundplatte weiß, Rahmen petrol (sitzt auf der Platte)
            color("white") box(g[links, vorne, boden], g[rechts, hinten, oben]);
            color("teal") difference() {
                box(g[links, vorne, oben] - [0, 0, 0.5], g[rechts, hinten, oben] + [0, 0, 1]);
                box(g[links, vorne, oben] + [rand, rand, -1], g[rechts, hinten, oben] - [rand, rand, -2]);
            }
            // Schrift-Relief rot, auf Filament 3 festgelegt
            color("red", filament = 3)
                for (i = [0:2]) translate(g[schrift, zeile, oben] + [i * 11, 0, -0.2]) buchstabe(["C", "A", "D"][i]);
        }
        // Aufhängebohrung: abgezogen, gilt für alle Farben
        translate(g[links, hinten, boden] + [6, -6, -1]) cylinder(h = t + 3, r = 1.6);
    }
}

module spielfigur(r = 9, h = 26) {
    grid p {
        x: achse = 0, hals = r * 0.3, rand = r * 0.85, fuss = r;
        y: boden = 0, sockel = 4, mitte = h * 0.55, kopf = h * 0.8;
    }
    // Profil: vom Rand des Sockels zum Hals (quadratisch), dann gerade bis zum Kopf
    z0 = p.sockel - 1;
    profil = concat(
        [[0, z0]],
        [for (i = [0:16]) let (t = i / 16) [p.hals + (p.rand - p.hals) * (1 - t) * (1 - t), z0 + t * (p.mitte - z0)]],
        [[p.hals, p.kopf], [0, p.kopf]]);
    color("blue") {
        cylinder(h = p.sockel, r = p.fuss);
        rotate_extrude() polygon(profil);
    }
    // weißer Kragen und gelber Kopf: spätere Anweisungen gewinnen bei Überlappung
    color("white") translate([0, 0, p.mitte - 1]) cylinder(h = 2, r = r * 0.5);
    color("yellow") translate([0, 0, p.kopf + 2.5]) sphere(r * 0.42);
}

// Spielfigur neben dem Schild (eigenes Bauteil = eigenes Objekt im Slicer)
module figur(at = [80, 14, 0]) translate(at) spielfigur();

schild: schild();
figur: figur();
