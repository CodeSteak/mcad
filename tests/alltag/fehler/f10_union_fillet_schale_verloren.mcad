// F10: union(fillet = 1) aus einer verrundeten Schale (shell) und einem Dom nahe der
// Innenecke: das Ergebnis ist nur der Dom samt Kehle, die Schale fehlt; Status ok,
// Volumen 0 mm³. Mit dem Dom bei (30, 30) stimmt das Ergebnis (~26 000 mm³).
// Status (Bericht 13): gemeldet statt still falsch (Verrundung liefert offenen Körper → Fehler,
// die Vereinigung bleibt); richtige Kehle offen (läuft auf die Bodenverrundung).
// SOLL: fehlerfrei
// SOLL-VOLUMEN-MIN: 20000
grid g {
    x: l = 0, r = 80;
    y: v = 0, h = 60;
    z: u = 0, o = 32;
}
union(fillet = 1) {
    shell(2) {
        fillet(3.5) {
            fillet(5) {
                box(g[l, v, u], g[r, h, o]);
                zone(g[l, v, *], d = 1);
                zone(g[r, v, *], d = 1);
                zone(g[l, h, *], d = 1);
                zone(g[r, h, *], d = 1);
            }
            zone(g.u, d = 1);
        }
        zone(g.o, d = 1);
    }
    translate([7.5, 7.5, 1.5]) cylinder(h = 30.5, d = 7);
}
