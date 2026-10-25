// F1: sweep entlang eines gekrümmten Pfads lehnt ein geschlossenes spline-Profil ab, obwohl es
// symmetrisch um den Ursprung liegt („profile that is not centred on its origin“). Die Prüfung
// in smlib/facade/sc_facade.cpp nimmt die Hülle der NURBS-Kurven und verlangt Abweichung
// ≤ 1e-9. Polygon und circle gehen. Umgehung: circle (Becher-Henkel).
// Status (Bericht 13): behoben – SMLib setzt den Mittelpunkt der Kontrollpunkt-Hülle auf den
// Pfad; smlib/patches/0003 lässt die Fassade den Ursprung des Profils vorgeben.
// SOLL: fehlerfrei
sweep(path = [[0, 0, 0], [20, 0, 10], [40, 0, 0]]) spline([[5, 0], [0, 3], [-5, 0], [0, -3]]);
