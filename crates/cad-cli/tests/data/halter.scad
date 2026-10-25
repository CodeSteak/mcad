// Zweck: -m 'modul(argumente)' – Modulkontext mit Argumenten wie im Code.
breite = 30;
function doppelt(x) = 2 * x;

module halter(w = 10, h = 2, d = 5) {
    cube([w, d, h]);
}

module teile() {
    module schraube(d = 2, l = 10) cylinder(h = l, d = d);
    schraube();
}

halter();
