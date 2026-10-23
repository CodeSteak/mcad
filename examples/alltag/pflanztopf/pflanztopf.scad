// Zweck: Pflanztopf mit Untersetzer (Baugruppe) – Topf als loft vom abgerundeten Quadrat
// (Superellipse als spline, Punkte aus einer Funktion) zur runden Öffnung, Füße und
// Abflusslöcher; Untersetzer als rotate_extrude mit shell oben geöffnet und über Zonen
// verrundet. Der Topf steht mit dem Anker `aufstand` auf dem Anker `boden` des Untersetzers.
// Prüfen: `cad measure pflanztopf.scad --interference --clearance topf untersetzer`. Maße in mm.
include <masse.scad>
use <teile.scad>

untersetzer: untersetzer();
translate(untersetzer.boden) topf: topf();
