// Zweck: SVG-Import (Plan 17.5, Bericht 16) – Logos und Icons aus examples/svg als exakte 2D-Profile.
// Kreise und Bögen bleiben Kreisbögen, Bézierkurven bleiben Kurven, Löcher nach fill-rule.
// Einheiten: viewBox + width/height in mm; ohne viewBox Pixel mit dpi (Standard 96).

dicke = 3;

// Logo, mittig um den Ursprung (center = true)
color("blue") linear_extrude(dicke) import("../svg/logo.svg", center = true);

// Blatt, auf 70 % verkleinert
translate([45, 0, 0])
    color("green") linear_extrude(dicke) import("../svg/blatt.svg", center = true, scale = 0.7);

// Haus als Prägung in einer Platte
translate([0, -50, 0]) difference() {
    color("silver") translate([-34, -27, 0]) cube([68, 54, dicke]);
    color("red") translate([0, 0, dicke - 1]) linear_extrude(2) import("../svg/haus.svg", center = true);
}

// Nur einzelne Elemente des SVG (id): ungleichmäßig skalierter Kreis (exakte Ellipsenbögen),
// gedrehter Ellipsenbogen
translate([70, -35, 0]) color("orange") linear_extrude(dicke) import("../svg/welle.svg", id = "oval", center = true);
translate([70, -60, 0]) color("teal") linear_extrude(dicke / 2) import("../svg/welle.svg", id = "bogen", center = true);
