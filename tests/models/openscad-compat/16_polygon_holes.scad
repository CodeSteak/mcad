// Zweck: polygon with holes via paths – perforated sheet/sign with two windows, extruded.
pts = [
    [0, 0], [40, 0], [40, 25], [0, 25],      // outside
    [5, 5], [15, 5], [15, 20], [5, 20],      // Window 1
    [25, 5], [35, 5], [30, 20]               // Window 2 (triangle)
];
linear_extrude(height = 2) polygon(points = pts, paths = [[0, 1, 2, 3], [4, 5, 6, 7], [8, 9, 10]]);
// Second part: frame of two paths without naming
translate([50, 0, 0]) linear_extrude(3)
    polygon([[0, 0], [20, 0], [20, 20], [0, 20], [4, 4], [16, 4], [16, 16], [4, 16]],
            [[0, 1, 2, 3], [4, 5, 6, 7]]);
