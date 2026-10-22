// Zweck: assert, Strings und str() – Namensschild-Rohling mit Parameterprüfung.
label = "Werkstatt";
w = len(label) * 6 + 10;
h = 15;
assert(w > 20, "Schild zu kurz");
assert(is_string(label));
echo(str("Schild ", label, ": ", w, " x ", h, " mm"));
echo(chars = [for (c = label) c], upper_first = ord(label[0]) < 97);
difference() {
    cube([w, h, 2]);
    for (x = [3, w - 3]) translate([x, h / 2, -1]) cylinder(h = 4, d = 2);
}
