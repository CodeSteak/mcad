// Zweck: -D ohne Modulkontext setzt Zuweisungen auf oberster Ebene (wie openscad -D).
w = 20;
d = w / 2;
h = 5;
h = 8; // die letzte Zuweisung gilt
cube([w, d, h]);
