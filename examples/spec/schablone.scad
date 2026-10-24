// Zweck: Sprühschablone mit der Standardschrift „cad stencil mono“ (Plan 17.4, Bericht 17) – der
// Text ist aus einer dünnen Platte ausgeschnitten; die Stege der Schrift halten die Innenteile
// von O, A, B, 0, 8 … an der Platte, nichts fällt heraus. Ohne `font` gilt die Schablonenschrift;
// `stroke_width` (Strichstärke) und `bridge_width` (Stegbreite) in mm, `bridge_width = 0` ohne Stege.

breite = 120;
hoehe = 44;
dicke = 1;
r = 4;              // Eckenradius
groesse = 12;       // Versalhöhe
strich = 1.7;       // Strichstärke (Standard 0,14 · size)
steg = 1.2;         // Stegbreite (Druck: mindestens zwei Düsenbreiten)

difference() {
    linear_extrude(dicke) offset(r = r) offset(delta = -r) square([breite, hoehe]);
    translate([breite / 2, 27, -1]) linear_extrude(dicke + 2)
        text("LACK 08", size = groesse, halign = "center", stroke_width = strich, bridge_width = steg);
    translate([breite / 2, 8, -1]) linear_extrude(dicke + 2)
        text("RAL 9005 B", size = 8, halign = "center", bridge_width = steg);
}
