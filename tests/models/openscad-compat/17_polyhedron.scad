// Zweck: polyhedron – pyramid and wedge with planar faces (faces clockwise seen from outside).
polyhedron(
    points = [[0, 0, 0], [10, 0, 0], [10, 10, 0], [0, 10, 0], [5, 5, 8]],
    faces = [[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4], [1, 0, 3, 2]]
);
translate([20, 0, 0]) polyhedron(
    [[0, 0, 0], [15, 0, 0], [15, 8, 0], [0, 8, 0], [0, 0, 6], [0, 8, 6]],
    [[0, 3, 2, 1], [0, 1, 4], [1, 2, 5, 4], [2, 3, 5], [3, 0, 4, 5]]
);
