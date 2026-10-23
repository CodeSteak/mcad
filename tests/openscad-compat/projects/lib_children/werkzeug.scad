versatz = 3;
function summe(v, i = 0) = i >= len(v) ? 0 : v[i] + summe(v, i + 1);
function abstand() = versatz * 2;
module reihe(n, d) {
    echo(n = n, kinder = $children);
    for (i = [0:n - 1]) translate([i * d + versatz, 0, 0]) children();
}
module fn_test() echo(fn = $fn, v = versatz);
reihe(2, 1) cube(1);
