// distinct geometry per tile (works around the shared-geometry colour bug)
names = ["red","orange","yellow","lime","green","teal","cyan","blue","purple","magenta","pink","brown","black","gray","silver","white"];
for (i = [0:15]) color(names[i]) translate([(i % 4) * 14, -floor(i / 4) * 14, 0]) cube([12 + i * 0.001, 12, 4]);
