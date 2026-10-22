// Zweck: polygon mit Löchern über paths – Lochblech/Schild mit zwei Fenstern, extrudiert.
pts = [
    [0, 0], [40, 0], [40, 25], [0, 25],      // außen
    [5, 5], [15, 5], [15, 20], [5, 20],      // Fenster 1
    [25, 5], [35, 5], [30, 20]               // Fenster 2 (Dreieck)
];
linear_extrude(height = 2) polygon(points = pts, paths = [[0, 1, 2, 3], [4, 5, 6, 7], [8, 9, 10]]);
// Zweites Teil: Rahmen aus zwei Pfaden ohne Benennung
translate([50, 0, 0]) linear_extrude(3)
    polygon([[0, 0], [20, 0], [20, 20], [0, 20], [4, 4], [16, 4], [16, 16], [4, 16]],
            [[0, 1, 2, 3], [4, 5, 6, 7]]);
