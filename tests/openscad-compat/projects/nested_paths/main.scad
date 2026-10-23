// Relative paths: include in a subfolder, which includes from a sibling folder; a used
// library that includes constants relative to itself.
include <lib/konst.scad>
use <lib/teile.scad>
echo(wand = wand, rand = rand);
echo(teil_breite(2));
teil();
echo(innen = innen);
for (i = [0:1]) zeige(i);
