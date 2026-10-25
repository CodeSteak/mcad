// Zweck: STL-Import (Plan 21, Phase B) – derselbe Winkel aus examples/stl einmal als B-Rep und
// einmal als reines Netz. Ein geschlossenes Netz mit wenigen Flächen wird automatisch zum
// B-Rep (Booleans, Verrundungen, Messen, STEP); `as = "mesh"` behält das Netz (Anzeige,
// Transformationen, Farben, Export STL/3MF, Messen – keine Booleans).

// B-Rep (automatisch: geschlossen, 8 Flächen) mit Bohrung im Fuß
color("blue") difference() {
    import("../stl/winkel.stl");
    translate([12, 5, -1]) cylinder(h = 5, r = 2);
}

// dasselbe Netz ohne B-Rep, gespiegelt daneben
translate([-5, 0, 0]) mirror([1, 0, 0]) color("orange") import("../stl/winkel.stl", as = "mesh");
