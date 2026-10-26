// Purpose: modifier ! – nothing but the marked subtree gets evaluated and exported.
cube(50);
translate([100, 0, 0]) ! union() {
    cube([10, 10, 2]);
    translate([5, 5, 2]) cylinder(h = 5, r1 = 4, r2 = 2);
}
sphere(30);
