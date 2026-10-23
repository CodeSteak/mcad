include <konst.scad>
innen = 100;
function teil_breite(n) = n * wand + innen;
module teil() echo(teil = wand, innen = innen);
module zeige(i) echo(i = i, basis = basis + i);
cube(wand);
