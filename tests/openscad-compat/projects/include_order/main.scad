use <a.scad>
module m() echo("own");
m();
x = 5;
include <c.scad>
echo(x=x, y=y);
cm();
echo(f());
