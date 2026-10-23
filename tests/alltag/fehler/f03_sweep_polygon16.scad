// F3: sweep eines 16-Ecks entlang eines gekrümmten Pfads: „sweep did not produce a closed
// solid (self-intersecting or open profile?)“. Dasselbe mit 32 Ecken oder circle() geht.
// ERWARTET: fehlerfrei
sweep(path = [[19, 0, 30], [35, 0, 25], [19, 0, 10]])
    polygon([for (i = [0 : 15]) 3 * [cos(i * 22.5), sin(i * 22.5)]]);
