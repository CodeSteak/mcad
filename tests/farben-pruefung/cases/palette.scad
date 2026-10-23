names = ["red","orange","yellow","lime","green","teal","cyan","blue","purple","magenta","pink","brown","black","gray","silver","white"];
for (i = [0:15]) color(names[i]) translate([(i % 4) * 14, -floor(i / 4) * 14, 0]) cube([12, 12, 4]);
