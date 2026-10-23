// F9b: ABSTURZ (Segmentation fault) von `cad check`: union(fillet = 1) aus einem Zapfen
// (Zylinder d 6) und einem gleich dicken sweep, der an der Stirnfläche des Zapfens beginnt
// (fast tangentiale Kreiskante). Mit union() kein Absturz. Im Schlüsselbrett-Haken umgangen.
// Status (Bericht 13): behoben – kein Absturz, Meldung zur Ecke (smlib/patches/0002).
// SOLL-FEHLER: cannot blend the corner
union(fillet = 1) {
    cylinder([0, 0, 0], [0, 8, 0], d = 6);
    sweep(path = [[0, 0, 0], [0, -13, -1], [0, -22, 4], [0, -23, 10]]) circle(d = 6);
}
