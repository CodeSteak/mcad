// F4 (klein): union(fillet) an einem Sweep mit 32-Eck-Profil scheitert (viele Facettenkanten);
// mit circle geht es (Schlüsselbrett-Haken).
// Status (Bericht 13): offen – der Sweep gelingt, die Verrundung der 32 Facettenkanten an der
// Kreiskante des Zapfens scheitert in SMLib (SM_ERR); Umgehung circle().
// SOLL: fehlerfrei
union(fillet = 1) {
    cylinder([0, 0, 0], [0, 8, 0], d = 8);
    sweep(path = [[0, 0, 0], [0, -13, -1], [0, -22, 4], [0, -23, 10]])
        polygon([for (i = [0 : 31]) 3 * [cos(i * 360 / 32), sin(i * 360 / 32)]]);
}
