// Zweck: vereinfachtes Stirnrad – Trapezzähne als polygon aus einer Listen-Comprehension.
z = 20;          // Zähnezahl
m = 2;           // Modul
r_p = z * m / 2; // Teilkreis
r_a = r_p + m;   // Kopfkreis
r_f = r_p - 1.25 * m;
step = 360 / z;
function pt(r, a) = [r * cos(a), r * sin(a)];
tooth = function (i) let(a = i * step) [
    pt(r_f, a - 0.35 * step), pt(r_a, a - 0.15 * step),
    pt(r_a, a + 0.15 * step), pt(r_f, a + 0.35 * step)
];
outline = [for (i = [0 : z - 1]) each tooth(i)];
echo(teeth = z, r_tip = r_a, points = len(outline));
difference() {
    linear_extrude(height = 6) polygon(outline);
    translate([0, 0, -1]) cylinder(h = 8, d = 5);
    for (i = [0 : 3]) rotate([0, 0, i * 90]) translate([r_f / 2 + 1.5, 0, -1]) cylinder(h = 8, r = 2);
}
