module halter(h = 5) { cube([2, 2, h]); anchor oben = [1, 1, h]; }
for (i = [0:2]) translate([i * 10, 0, 0]) k[i]: halter(h = 5 + i);
translate(k[1].oben) sphere(1);
