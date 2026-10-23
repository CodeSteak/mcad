// Zweck: Elektronikgehäuse mit Deckel und vier Schrauben (Baugruppe aus drei Dateien) –
// Maße per include (masse.scad), Bauteile per use (teile.scad). Unterteil mit shell und
// Domen (benannte Instanzen dom[i] im Modul), Deckel am Anker des Unterteils, Schrauben als
// Bauteile s[0] … s[3] in einer Schleife. `explosion` > 0 zieht Deckel und Schrauben
// auseinander (Ansicht); bei 0 ist die Baugruppe geschlossen (Passung prüfen:
// `cad measure gehaeuse.scad --interference --clearance unterteil deckel`). Maße in mm.
include <masse.scad>
use <teile.scad>

explosion = 0;

unterteil: unterteil();
translate(unterteil.deckel_auflage + [0, 0, explosion]) deckel: deckel();
for (i = [0 : 3])
    translate([dom_pos(i).x, dom_pos(i).y, hoehe + deckel_t + 2 * explosion]) s[i]: schraube();
