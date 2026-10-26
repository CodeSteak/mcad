// Zweck: Lochrasterplatte – eine for-Schleife mit zwei Variablen erzeugt ein Bohrungsraster.
nx = 6;
ny = 4;
pitch = 10;
t = 3;
echo(holes = nx * ny);
difference() {
    cube([nx * pitch, ny * pitch, t]);
    for (i = [0 : nx - 1], j = [0 : ny - 1])
        translate([pitch / 2 + i * pitch, pitch / 2 + j * pitch, -1])
            cylinder(h = t + 2, d = 4);
}
