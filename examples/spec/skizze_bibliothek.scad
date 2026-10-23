// Zweck: Module als Namensräume (Plan 17.2) – eine Bibliothek `profile` mit Skizzen-Modulen
// und eine Teilebibliothek `teile` mit Konstanten, Funktionen und Untermodulen; Aufrufe von
// außen mit Punkt (`profile.stern()`, `teile.schraube(d = 3)`, mehrstufig
// `teile.platte.lochplatte()`), Funktionen und Konstanten als `teile.kopf(teile.M3)`.
module profile {
    module stern(n = 5, r1 = 12, r2 = 5) sketch {
        start([r1, 0]);
        for (i = [1 : n]) {
            line(to = [r2 * cos(360 * (i - 0.5) / n), r2 * sin(360 * (i - 0.5) / n)]);
            line(to = [r1 * cos(360 * i / n), r1 * sin(360 * i / n)]);
        }
    }
    module langloch(l = 20, b = 6) sketch {
        start([0, -b / 2]);
        line(to = [l, -b / 2]);
        arc(to = [l, b / 2], r = b / 2);
        line(to = [0, b / 2]);
        arc(to = [0, -b / 2], r = b / 2);
    }
}

module teile(raster = 10) {
    M3 = 3.2;                                   // Durchgangsbohrung M3
    function kopf(d) = 1.8 * d;                 // Kopfdurchmesser
    module schraube(d = 3, l = 12) {
        cylinder(h = l, d = d);
        translate([0, 0, l]) cylinder(h = d * 0.6, d = kopf(d));
    }
    module platte {
        // sieht `raster` und `M3` von `teile` (Standardwerte)
        module lochplatte(n = 3) {
            difference() {
                cube([n * raster, raster, 3]);
                for (i = [0 : n - 1]) translate([raster * (i + 0.5), raster / 2, -1]) cylinder(h = 5, d = M3);
            }
        }
    }
}

linear_extrude(3) profile.stern();
translate([25, -3, 0]) linear_extrude(4) profile.langloch(l = 25);
translate([0, 30, 0]) teile.platte.lochplatte(n = 4);
for (i = [0 : 3]) translate([10 * (i + 0.5), 35, 3]) teile.schraube(d = 3, l = 8);
translate([70, 0, 0]) cylinder(h = 2, d = teile.kopf(teile.M3));
