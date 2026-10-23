// Maße des Schlüsselbretts (per include): Brett, Haken, Passung Zapfen ↔ Bohrung
brett_l = 200;
brett_h = 70;
brett_t = 12;
anzahl = 5;              // Haken
haken_z = 22;            // Höhe der Hakenreihe über der Unterkante
zapfen_d = 6;            // Zapfen am Haken (eingeklebt)
zapfen_l = 8;
bohrung_d = 6.3;         // Bohrung im Brett (0,15 mm Spiel radial für Kleber)

// x-Position von Haken i (gleichmäßig verteilt, Rand je eine halbe Teilung)
function haken_x(i) = brett_l / anzahl * (i + 0.5);
