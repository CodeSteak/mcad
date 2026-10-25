// Skizze (Rechteck mit Halbkreis) und ein Namensraum mit Untermodul und Funktion
module teile {
    function flaeche(a, b, r) = a * b + PI * r * r / 2;
    module platte(a = 40, b = 20, h = 5) {
        linear_extrude(h) sketch {
            start([0, 0]);
            line(to = [a, 0]);
            line(to = [a, b]);
            arc(to = [0, b], r = a / 2);
            close();
        }
    }
}

teile.platte();
echo(teile.flaeche(40, 20, 20));
