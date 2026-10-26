names = ["red","orange","yellow","lime","green","teal","cyan","blue"];
for (i = [0:39]) color(names[i % 8]) translate([i * 3, 0, 0]) cube([4 + i*0.001, 5, 5]);
