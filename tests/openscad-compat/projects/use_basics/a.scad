module m() echo("a");
function f() = "a";
la = 1;
function getla() = la;
module showla() echo(la=la, fn=$fn);
cube(1);
use <lib/inner.scad>
module callinner() inner();
