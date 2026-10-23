// Library modules with children, $-variables from the caller, functions calling each other
// and recursion inside the library.
use <werkzeug.scad>
reihe(3, 5) sphere(1);
echo(summe([1, 2, 3, 4]));
echo(abstand(), $fn = 7);
fn_test($fn = 12);
echo(versatz = versatz);
