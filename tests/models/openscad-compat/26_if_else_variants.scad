// Zweck: if/else und ?: wählen Varianten eines Knopfes (rund, eckig, mit/ohne Bohrung).
module knob(style = "round", hole = true, size = 10) {
    difference() {
        if (style == "round") cylinder(h = 6, d = size);
        else if (style == "square") translate([-size / 2, -size / 2, 0]) cube([size, size, 6]);
        else sphere(size / 2);
        if (hole) translate([0, 0, -1]) cylinder(h = 8, d = size > 8 ? 3 : 2);
    }
}
knob();
translate([15, 0, 0]) knob("square", false);
translate([30, 0, 0]) knob(style = "square", size = 8);
translate([45, 0, 0]) knob("ball", hole = false, size = 8);
