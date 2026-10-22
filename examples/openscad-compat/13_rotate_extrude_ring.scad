// Zweck: rotate_extrude eines verschobenen Kreises (Ring) und eines Rechtecks (Scheibe mit Loch).
rotate_extrude() translate([10, 0]) circle(r = 2);
translate([30, 0, 0]) rotate_extrude() translate([4, 0]) square([3, 5]);
