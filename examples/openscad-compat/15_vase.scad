// Zweck: Vase als rotate_extrude eines Polygonprofils (Wand 2 mm, Boden 2 mm).
h = 40;
wall = 2;
outer = [[12, 0], [15, 10], [11, 25], [9, 35], [11, h]];
// Innenkontur: Außenkontur um die Wandstärke nach innen, von oben nach unten.
inner = [for (i = [len(outer) - 1 : -1 : 1]) [outer[i][0] - wall, max(outer[i][1], wall)]];
profile = concat([[0, 0]], outer, inner, [[outer[0][0] - wall, wall], [0, wall]]);
echo(points = len(profile), top = profile[len(outer)]);
rotate_extrude() polygon(profile);
