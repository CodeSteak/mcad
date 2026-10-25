// Maße des Elektronikgehäuses (per include eingebunden: Variablen und Funktionen gelten in
// der Hauptdatei). Innenmaße für eine Platine 70 × 50 mm, Schrauben M3 (selbstschneidend).
innen_l = 76;          // Innenlänge
innen_b = 56;          // Innenbreite
hoehe = 32;            // Höhe des Unterteils (außen)
wand = 2;              // Wandstärke
ecke = 5;              // Radius der senkrechten Außenkanten
deckel_t = 2.5;        // Deckelstärke
rand_h = 3;            // Höhe des Zentrierrands am Deckel
spiel = 0.2;           // Spiel Zentrierrand ↔ Innenwand
dom_d = 7;             // Schraubdom außen
kern_d = 2.5;          // Kernloch im Dom (M3 in Kunststoff)
durch_d = 3.4;         // Durchgangsloch im Deckel
schraube_l = 12;       // Schraubenlänge unter dem Kopf

aussen_l = innen_l + 2 * wand;
aussen_b = innen_b + 2 * wand;

dom_abstand = 3;       // Luft Dom ↔ Innenwand: Domfuß samt Kehle bleibt auf dem ebenen Boden (README: F9, F10)

// Mitte des Schraubdoms i (0 … 3) im System des Unterteils, in den Innenecken
function dom_pos(i) = let (a = wand + dom_abstand + dom_d / 2)
    [i % 2 == 0 ? a : aussen_l - a, i < 2 ? a : aussen_b - a];
