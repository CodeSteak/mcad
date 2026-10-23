// Zweck: Zahnriemenscheibe (GT2-ähnlich) als Skizze per Schleife (Plan 17.1): jede Zahnlücke
// ist ein Bogen durch einen Punkt am Lückengrund, dazwischen ein Bogen um die Mitte auf dem
// Kopfkreis; die Nabenbohrung ist eine zweite Kontur im Uhrzeigersinn. Bordscheiben als Zylinder.
function polar(r, a) = [r * cos(a), r * sin(a)];

module scheibe(z = 20, teilung = 2, h = 7, bohrung = 5) {
    ra = z * teilung / PI / 2 - 0.25;           // Kopfkreisradius
    t = 0.75;                                   // Tiefe der Zahnlücke
    dw = 0.6 / ra * 180 / PI;                   // halbe Lückenbreite als Winkel
    a = 360 / z;
    translate([0, 0, 1]) linear_extrude(h) sketch {
        start(polar(ra, -dw));
        for (i = [0 : z - 1]) {
            arc(through = polar(ra - t, i * a), to = polar(ra, i * a + dw));
            arc(center = [0, 0], to = polar(ra, (i + 1) * a - dw));
        }
        start([bohrung / 2, 0]);
        arc(to = [-bohrung / 2, 0], r = bohrung / 2, cw = true);
        arc(to = [bohrung / 2, 0], r = bohrung / 2, cw = true);
    }
    for (zb = [0, h + 1]) translate([0, 0, zb]) difference() {
        cylinder(h = 1, r = ra + 1);
        translate([0, 0, -1]) cylinder(h = 3, d = bohrung);
    }
}

scheibe();
