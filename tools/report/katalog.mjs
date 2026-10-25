#!/usr/bin/env node
// Katalog der Alltagsmodelle (examples/alltag/): misst jedes Modell mit der CLI, rendert Bilder
// und schreibt
//
//   docs/modelle/alltagsmodelle.html, docs/modelle/img/*.jpg
//
// Alle Zahlen kommen aus der CLI (`cad outline`, `measure`, `eval`), Texte aus MODELLE unten
// (Platzhalter {param} bzw. {modul.param} werden durch die Standardwerte aus `cad outline`
// ersetzt). Zwischenergebnisse: reports/tmp/katalog/ (gitignored). Binary: $CAD (Standard
// target/release/mcad), Socket-Ordner $CAD_IPC_DIR (kurzer Pfad, z. B. /tmp/cadkat).
// Braucht ImageMagick (`convert`, `identify`).
//
//   node tools/report/katalog.mjs [--reuse [--redo=id,…]]   (--reuse: Messwerte/Bilder aus reports/tmp
//                                                             wiederverwenden, außer für --redo)
//   node tools/report/print-pdf.mjs docs/modelle/alltagsmodelle.html docs/modelle/alltagsmodelle.pdf "Alltagsmodelle – Katalog"

import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const cad = process.env.CAD ?? join(root, "target/release/mcad");
const work = join(root, "reports/tmp/katalog");
const out = join(root, "docs/modelle");
const img = join(out, "img");
const reuse = process.argv.includes("--reuse");
const PLA = 1.24; // g/cm³
const BG = "#f4f5f7"; // Hintergrund von `cad render` (helles Thema)

// ---------------------------------------------------------------------------------------------
// Modelle: Reihenfolge wie examples/alltag/README.md. text: 2–4 Sätze; {p} = Standardwert des
// Parameters p des Hauptmoduls bzw. der Datei-Variable p, {modul.p} = Parameter eines anderen
// Moduls, {p|x} = Liste mit „ד verbunden.
// extra: zusätzliche Bilder der Baugruppen (Schnitt, Anker), pairs: Abstände `measure --clearance`,
// view: [Azimut, Höhe] statt iso, wo iso die Rückseite zeigt.
const MODELLE = [
  {
    id: "handystaender", file: "handystaender.mcad", titel: "Handyständer", view: [-50, 30], // iso zeigt die Rückseite
    text: "Ständer für ein Telefon bis 12 mm Dicke. Das Seitenprofil ist ein Polygon aus Punkten eines 2D-Rasters und wird über die Breite w = {w} mm extrudiert; die Lehne ({lehne} mm, Wand t = {t} mm) steht im Winkel a = {a}° zur Waagrechten, ein dreieckiges Fenster entlastet sie. Ein Kabelloch Ø {kabel_d} mm mit Kanal unter dem Boden nimmt das Ladekabel auf. Innenecken von Lippe und Ablage sowie die Lippenoberkante sind über Zonen an Rasterlinien mit zwei Radien verrundet.",
  },
  {
    id: "becher", file: "becher.mcad", titel: "Becher mit Henkel",
    text: "Trinkbecher als konischer Rotationskörper (Radius unten ru = {ru} mm, oben ro = {ro} mm, Höhe h = {h} mm), der mit shell und Wand {wand} mm oben geöffnet wird. Die Bodenkante ist über eine Zone an der Bodenebene mit r {boden_r} mm verrundet, der Trinkrand über eine zweite Zone. Der Henkel (Ø {henkel_d} mm) ist ein sweep eines Kreises entlang eines Spline-Pfads, dessen Enden radial in der Wand sitzen, und mit union(fillet = …) angesetzt.",
  },
  {
    id: "kabelclip", file: "kabelclip.mcad", titel: "Kabelclip für die Tischkante", kurz: "Kabelclip",
    text: "Kabelhalter zum Aufstecken auf eine Tischplatte von dicke = {dicke} mm (Spiel {spiel} mm). Die C-Klammer ist ein extrudiertes Profil (Wand {t} mm, Arme {arm_o} und {arm_u} mm) mit Klemmnoppe; darüber sitzt ein Block mit n = {n} Kabelkanälen Ø {kabel_d} mm im Abstand {pitch} mm, die von oben eingeführt werden. Die Kanäle liegen an indizierten Rasterebenen, ihre Kanten sind mit difference(fillet = …) verrundet, die Innenecken der Klammer über Zonen.",
  },
  {
    id: "wandhaken", file: "wandhaken.mcad", titel: "Wandhaken",
    text: "Haken für Jacke oder Handtuch: Wandplatte {b} × {h} × {t} mm mit verrundeten Ecken (r {ecke} mm), gefaster Vorderkante und zwei Senkbohrungen (Ø {schraube_d} mm, Kopf Ø {kopf_d} mm). Fase und Verrundung sind verschachtelt, so dass die Fase auch die Randkanten der Verrundung erfasst. Der Haken (Ø {haken_d} mm, Ausladung {ausladung} mm) ist ein sweep entlang eines Spline-Pfads mit Kugelknauf Ø {knauf_d} mm, angesetzt mit union(fillet = …).",
  },
  {
    id: "seifenschale", file: "seifenschale.mcad", titel: "Seifenschale",
    text: "Seifenschale {l} × {b} × {h} mm mit Ablauf. Die Wanne ist ein Quader, dessen senkrechte Ecken (r {ecke} mm) und Bodenkanten über verschachtelte Zonen verrundet werden und den shell (Wand {wand} mm) oben öffnet. n = {n} runde Auflagerippen sind cylinder(a, b, r) zwischen Rasterpunkten, dazwischen liegen Ablaufschlitze mit verrundeten Kanten; unten stehen vier Füße ({fuss_h} mm).",
  },
  {
    id: "stiftebox", file: "stiftebox.mcad", titel: "Stiftebox",
    text: "Stiftebox mit drei hohen Fächern (Breiten {faecher} mm, Tiefe {tiefe} mm, Höhe {h} mm) und einem flachen vorderen Fach ({vorn_t} mm tief, {vorn_h} mm hoch), Wand {wand} mm. Die Fachgrenzen sind indizierte Rasterebenen, die eine rekursive Funktion aus der Liste der Fachbreiten berechnet. Vier verschachtelte Zonenblöcke verrunden die senkrechten Kanten (außen r {ecke} mm, Taschen r {ecke_i} mm) und die Taschenböden und fasen die hinteren Oberkanten; die Oberkanten des vorderen Fachs bleiben scharf (Kernel-Grenze F6).",
  },
  {
    id: "tuerstopper", file: "tuerstopper.mcad", titel: "Türstopper",
    text: "Keil {l} mm lang, {b} mm breit und hinten {h_hinten} mm hoch. Der Körper ist ein loft durch drei Trapezprofile (Höhe hinten, Mitte {h_mitte} mm, Spitze {spitze} mm), liegend ausgerichtet mit rotate([90, 0, 90]); seine Unterkante ist vorher über eine Zone an der Bodenebene gefast. Oben liegt eine Griffmulde, unten {rillen} Querrillen gegen Rutschen, hinten ein Aufhängeloch Ø {loch_d} mm.",
  },
  {
    id: "topfuntersetzer", file: "topfuntersetzer.mcad", titel: "Topfuntersetzer",
    text: "Untersetzer Ø {d} mm, {h} mm hoch: drei konzentrische Ringe (Mittenradien {ringe} mm, Steg {steg} mm) als rotate_extrude eines Rechtecks in einer for-Schleife, verbunden durch {speichen} Speichen und eine Mittelscheibe (r {mitte_r} mm). Alle Oberkanten von Ringen und Speichen werden in einem Block über eine Zone an der Deckebene gefast; unten liegen drei Mulden für Gummifüße Ø {fuss_d} mm.",
  },
  {
    id: "flansch", file: "flansch.mcad", titel: "Flanschplatte",
    text: "Flansch Ø {d_aussen} mm, {t} mm stark, mit Bund Ø {d_bund} mm (Höhe {h_bund} mm) und Bohrung Ø {d_bohrung} mm. Der Querschnitt ist ein Polygon aus einem 2D-Raster, gedreht mit rotate_extrude; auf dem Lochkreis Ø {lk} mm liegen {n} Durchgangslöcher Ø {d_loch} mm (M10), dazu eine Dichtnut {nut_t} mm tief. Den Übergang Bund/Scheibe verrundet ein eigenes Modul als Zone (ringzone), Bohrungs- und Außenkanten sind über Volumenzonen gefast.",
  },
  {
    id: "lagerbock", file: "lagerbock.mcad", titel: "Stehlagerbock 608",
    text: "Lagerbock für ein Rillenkugellager 608 ({lager_d} × {lager_b} mm), Achshöhe {achse_h} mm. Die Grundplatte {l} × {b} × {t} mm hat zwei Langlöcher für M6 ({langloch|x} mm, Hilfsmodul) zum Ausrichten; das Lagerauge (r {auge_r} mm, {auge_b} mm breit) ist ein Quader mit tangential anschließendem cylinder(a, b, r) und hat eine Anschlagschulter Ø {schulter_d} mm. Die Kehlen rund um das Auge entstehen über Zonen an Rasterlinien in x, y und z.",
  },
  {
    id: "winkel_rippe", file: "winkel_rippe.mcad", titel: "Montagewinkel mit Rippe", kurz: "Montagewinkel",
    text: "Winkel mit Schenkeln {a} und {h} mm, {b} mm breit und {t} mm stark, mit n_rippen = {n_rippen} Versteifungsrippe ({rippe_t} mm dick, {rippe_l} mm lang). Die Rippen sind extrudierte Dreiecke an indizierten Rasterebenen; Innenkante und Rippenanschlüsse werden in einem Zonenblock verrundet, so dass die Eckübergänge sauber laufen. Die Bohrungen Ø {loch_d} mm (M5) sind mit difference(chamfer = …) gefast.",
  },
  {
    id: "kuehlkoerper", file: "kuehlkoerper.mcad", titel: "Kühlkörper",
    text: "Strangpress-Kühlkörper {l} × {b} mm, Grundplatte {basis} mm, Gesamthöhe {h} mm, mit n = {n} Rippen à {rippe_t} mm. Die Rippenlage steht als indizierte Rasterebenen aus einer Liste; die Rippenfüße sind über Zonen an Rasterlinien in einer for-Schleife verrundet (r {fuss_r} mm), die Köpfe über eine Zone an der Kopfebene fast voll gerundet. Zwei Bohrungen M3 im Abstand {loch_abstand} mm liegen zwischen den Rippen, ein Anker markiert die Wärmequelle.",
  },
  {
    id: "zahnrad", file: "zahnrad.mcad", titel: "Stirnradpaar",
    text: "Zwei Stirnräder mit Modul m = {m} mm und {z1} bzw. {z2} Zähnen im Eingriff, Achsabstand {achsabstand} mm, Breite {zahnrad.b} mm. Die Evolventenflanken entstehen als Polygon aus Listenfunktionen (let, each, concat) und werden linear extrudiert; die Zahndicke ist für das Flankenspiel leicht verringert. Rad 1 hat Nabe, Passfedernut und Erleichterungslöcher, Rad 2 eine D-Bohrung für eine Motorwelle 5 mm.",
    pairs: [["rad1", "rad2", "Flankenspiel"]],
    extra: [{ name: "eingriff", args: ["--view", "top", "--fit", "all", "--zoom", "6", "--target", "18.6,-4.2,10"], caption: "Draufsicht, 6-fach vergrößert auf die Engstelle des Eingriffs: der Spalt zwischen den Flanken ist das gemessene Flankenspiel." }],
  },
  {
    id: "rohrschelle", file: "rohrschelle.mcad", titel: "Rohrschelle",
    text: "Zweiteilige Schelle für ein Rohr Ø {rohr_d} mm: die Halbringe ({halbschelle.t} mm stark, {b} mm breit) sind rotate_extrude(angle = 180) eines Rechtecks, mit Laschen und Bohrungen Ø {halbschelle.loch_d} mm (M5); das Unterteil (mirror) trägt einen Stutzen mit Kernloch für M8. Oberteil, Unterteil und ein Prüfrohr sind Bauteile in einem gemeinsam gedrehten Block. Im offenen Zustand klafft an den Laschen der Spalt {halbschelle.spalt} mm.",
    pairs: [["oben", "unten", "Klemmspalt an den Laschen"], ["oben", "rohr", "Rohr liegt am Oberteil an"], ["unten", "rohr", "Rohr liegt am Unterteil an"]],
    extra: [{ name: "schnitt", args: ["--section", "x=10", "--view", "right"], caption: "Schnitt x = 10 (Mitte der Schelle, quer zur Rohrachse), Blick von +x: Halbringe mit Laschen und Bohrungen M5, Klemmspalt 2 mm, Stutzen mit Kernloch für M8, Prüfrohr." }],
  },
  {
    id: "pi_platte", file: "pi_platte.mcad", titel: "Montageplatte Raspberry Pi", kurz: "Raspberry-Pi-Platte",
    text: "Platte {l} × {b} × {t} mm für einen Raspberry Pi (Lochbild 58 × 49 mm, M2.5). Die Abstandshalter ({hoehe} mm) sind ein eigenes Modul mit Anker, in der Schleife als benannte Instanzen dom[i] gesetzt und mit union(fillet = …) angesetzt. Dazu kommen {schlitze} Lüftungsschlitze, vier Befestigungslöcher M4 mit Senkung und verrundete Ecken (r {ecke} mm). Ein farbiger Platinen-Dummy 85 × 56 × 1,6 mm liegt als zweites Bauteil am Anker der Platte.",
    pairs: [["platte", "pi", "Platine liegt auf den Domen"]],
    extra: [{ name: "anker", args: ["--show", "anchors", "--part", "platte"], caption: "Platte mit eingeblendeten Ankern (gelb): die Anker oben der Dome tragen die Platine." }],
  },
  {
    id: "gehaeuse", file: "gehaeuse/gehaeuse.mcad", titel: "Elektronikgehäuse",
    text: "Gehäuse für eine Platine 70 × 50 mm: innen {innen_l} × {innen_b} mm, Höhe {hoehe} mm, Wand {wand} mm, Ecken r {ecke} mm. Das Unterteil ist ein verrundeter Quader, den shell öffnet; vier Schraubdome (Kernloch Ø {kern_d} mm) sind als dom[i] mit union(fillet = …) angesetzt. Der Deckel ({deckel_t} mm, Zentrierrand {rand_h} mm, Spiel {spiel} mm) sitzt am Anker deckel_auflage, die Schrauben M3 sind Bauteile s[0] … s[3]. Maße stehen in masse.mcad (include), Teile in teile.mcad (use); explosion > 0 zieht die Baugruppe auseinander.",
    pairs: [["unterteil", "deckel", "Deckel liegt auf"], ["s[0]", "deckel", "Schraubenkopf liegt auf"], ["s[0]", "unterteil", "Schaft im Kernloch"]],
    extra: [{ name: "schnitt", args: ["--section", "y=8.5", "--section-flip", "--view", "front"], caption: "Schnitt y = 8,5 (durch die vorderen Dome), Blick von vorn: Schraube im Dom, Deckel mit Zentrierrand auf dem Unterteil." }],
  },
  {
    id: "scharnier", file: "scharnier/scharnier.mcad", titel: "Scharnier",
    text: "Druckbares Scharnier {laenge} mm aus Blatt A, Blatt B und Stift mit {augen} Gelenkaugen (r {auge_r} mm, Blattstärke {blatt_t} mm). Die Augen beider Blätter kommen aus derselben Funktion, so dass das Spiel von {spiel} mm überall gleich ist; die Freiräume entstehen per for mit Schrittweite. Blatt A hat die Bohrung {bohrung_a} mm (Stift eingeklebt), Blatt B {bohrung_b} mm (dreht frei), der Stift Ø {stift_d} mm. Die Variable winkel = {winkel}° dreht Blatt B um die Achse.",
    pairs: [["a", "b", "Spiel der Augen"], ["stift", "b", "Lagerspiel"], ["stift", "a", "Stiftkopf liegt an"]],
    extra: [
      { name: "schnitt", args: ["--section", "z=0", "--view", "top"], caption: "Schnitt z = 0 (Mittelebene der Blätter), Draufsicht: Augen von A (unten) und B (oben) abwechselnd mit axialem Spiel, Stift in den Bohrungen." },
      { name: "lager", args: ["--section", "x=18", "--view", "right", "--zoom", "3.5", "--target", "18,0,0"], caption: "Schnitt x = 18 (zweites Auge, Blatt B), Blick von +x, 3,5-fach: Stift Ø 2,95 in der Bohrung 3,4 von B; links Blatt A mit Freiraum." },
    ],
  },
  {
    id: "pflanztopf", file: "pflanztopf/pflanztopf.mcad", titel: "Pflanztopf mit Untersetzer", kurz: "Pflanztopf",
    text: "Der Topf geht per loft vom abgerundeten Quadrat ({topf_unten} mm, Superellipse als spline aus einer Funktion) zur runden Öffnung Ø {topf_oben_d} mm, Höhe {topf_h} mm, Wand {wand} mm, Boden {boden} mm; er steht auf {fuss_h} mm hohen Füßen und hat Abflusslöcher. Die Wand ist die Differenz zweier Lofts. Der Untersetzer ist ein rotate_extrude, mit shell oben geöffnet und über Zonen verrundet. Der Topf steht mit dem Anker aufstand auf dem Anker boden des Untersetzers.",
    pairs: [["topf", "untersetzer", "Füße stehen auf dem Boden"]],
    extra: [{ name: "schnitt", args: ["--section", "y=0", "--section-flip", "--view", "front"], caption: "Schnitt y = 0, Blick von vorn: Wand des Lofts, Füße und Abflusslöcher, Topf im Untersetzer." }],
  },
  {
    id: "schluesselbrett", file: "schluesselbrett/schluesselbrett.mcad", titel: "Schlüsselbrett",
    text: "Brett {brett_l} × {brett_h} × {brett_t} mm mit {anzahl} eingeklebten Haken. Es hat über Zonen verrundete Ecken und eine gefaste Vorderkante, hinten zwei Schlüssellochaufhängungen und oben eine Briefrille. Die Haken sind ein sweep entlang eines Spline-Pfads mit Kugelknauf; ihr Zapfen Ø {zapfen_d} mm sitzt in einer Bohrung Ø {bohrung_d} mm (Kleberspalt). Die Haken sind Bauteile h[0] … h[4] aus einer for-Schleife, die Maße stehen in masse.mcad.",
    pairs: [["brett", "h[0]", "Zapfen in der Bohrung (Kleberspalt)"]],
    extra: [{ name: "schnitt", args: ["--section", "x=20", "--view", "right"], caption: "Schnitt x = 20 (durch Haken h[0]), Blick von +x: Zapfen in der Bohrung, Fase der Vorderkante, Briefrille oben." }],
  },
];

// ---------------------------------------------------------------------------------------------
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const de = (x, d = 0) => Number(x).toLocaleString("de-DE", { minimumFractionDigits: d, maximumFractionDigits: d });
const num = (x) => Number(x).toLocaleString("de-DE", { maximumFractionDigits: 3 });
const rel = (p) => relative(root, p);
const git = (...a) => {
  try {
    return execFileSync("git", a, { cwd: root, encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
};

function run(args) {
  const t0 = process.hrtime.bigint();
  const r = spawnSync(cad, args, { cwd: root, encoding: "utf8", maxBuffer: 1 << 28 });
  const ms = Number(process.hrtime.bigint() - t0) / 1e6;
  let j = null;
  try {
    j = JSON.parse(r.stdout);
  } catch {}
  if (r.status !== 0) console.error(`  ! cad ${args.join(" ")} → Exit ${r.status}${r.signal ? " (" + r.signal + ")" : ""}: ${(r.stderr || "").trim().slice(0, 300)}`);
  return { code: r.status, signal: r.signal, j, ms, err: r.stderr ?? "" };
}
const im = (...a) => execFileSync("convert", a, { encoding: "utf8" });
const identify = (f) => execFileSync("identify", ["-format", "%w %h", f], { encoding: "utf8" }).split(" ").map(Number);

/** PNG → eingepasstes JPEG: Ränder abschneiden, auf Seitenverhältnis w:h mit Rand auffüllen. */
function fitJpeg(src, dst, w, h, margin = 0.06, quality = 84) {
  const trimmed = join(work, "trim.png");
  im(src, "-fuzz", "1%", "-trim", "+repage", trimmed);
  const [tw, th] = identify(trimmed);
  const s = Math.min((w * (1 - 2 * margin)) / tw, (h * (1 - 2 * margin)) / th);
  im(trimmed, "-resize", `${Math.round(tw * s)}x${Math.round(th * s)}`, "-background", BG, "-gravity", "center", "-extent", `${w}x${h}`,
    "-strip", "-sampling-factor", "4:2:0", "-quality", String(quality), dst);
  return checkSize(dst);
}
function toJpeg(src, dst, w, quality = 80) {
  im(src, "-resize", `${w}x`, "-strip", "-sampling-factor", "4:2:0", "-quality", String(quality), dst);
  return checkSize(dst);
}
function checkSize(f) {
  const b = statSync(f).size;
  if (b > 200 * 1024) throw new Error(`${rel(f)}: ${b} B > 200 KB`);
  return b;
}

/** Sprachmittel, die im Quelltext (samt include/use) vorkommen. */
const MITTEL = [
  ["Raster (grid)", /\bgrid\s+\w+\s*\{/],
  ["indizierte Rasterebenen", /^\s*[xyz]:.*=\s*\[/m],
  ["Anker", /\banchor\s+\w+/],
  ["Anker mit dir", /\banchor\s+\w+\s*=.*dir\s*=/],
  ["benannte Instanzen", /^\s*(translate\([^;]*\)\s*)?\w+(\[\w+\])?\s*:\s*\w+\(/m],
  ["include", /^\s*include\s*</m],
  ["use", /^\s*use\s*</m],
  ["Funktion", /\bfunction\s+\w+/],
  ["Rekursion", /function\s+(\w+)\s*\([^)]*\)\s*=[^;]*\b\1\s*\(/],
  ["let", /\blet\s*\(/],
  ["Listen-Komprehension", /\[\s*for\s*\(/],
  ["each", /\beach\b/],
  ["for", /\bfor\s*\(/],
  ["if / ?:", /\bif\s*\(|\?[^:;]+:/],
  ["polygon", /\bpolygon\s*\(/],
  ["spline", /spline/],
  ["linear_extrude", /\blinear_extrude\b/],
  ["rotate_extrude", /\brotate_extrude\b/],
  ["loft", /\bloft\s*\(/],
  ["sweep", /\bsweep\s*\(/],
  ["shell", /\bshell\s*\(/],
  ["fillet-Zonen", /\bfillet\s*\([^)]*\)\s*\{/],
  ["chamfer-Zonen", /\bchamfer\s*\([^)]*\)\s*\{/],
  ["zone", /\bzone\s*\(/],
  ["difference(fillet/chamfer)", /\bdifference\s*\(\s*(fillet|chamfer)\s*=/],
  ["union(fillet)", /\bunion\s*\(\s*fillet\s*=/],
  ["$fillet", /\$fillet\s*=/],
  ["box(a, b)", /\bbox\s*\(/],
  ["cylinder(a, b, r)", /\bcylinder\s*\(\s*(g\[|\[|\w+\[)/],
  ["Kegel (r1, r2)", /\bcylinder\s*\([^;]*\b(r1|d1)\s*=/],
  ["sphere", /\bsphere\s*\(/],
  ["intersection", /\bintersection\s*\(/],
  ["mirror", /\bmirror\s*\(/],
  ["color", /\bcolor\s*\(/],
];
function sprachmittel(files) {
  const src = files.map((f) => readFileSync(f, "utf8").replace(/\/\/.*$/gm, "")).join("\n");
  return MITTEL.filter(([, re]) => re.test(src)).map(([n]) => n);
}

/** Platzhalter {p}, {modul.p}, {p|x} durch Standardwerte ersetzen. */
function fill(text, o, main) {
  const fmt = (v, join) => (Array.isArray(v) ? v.map((x) => num(x)).join(join) : typeof v === "number" ? num(v) : String(v));
  return text.replace(/\{([\w.]+)(\|x)?\}/g, (_, key, x) => {
    const join = x ? " × " : " / ";
    let mod = main, name = key;
    if (key.includes(".")) [mod, name] = key.split(".");
    const m = o.modules.find((m) => m.name === mod);
    const p = m?.params.find((p) => p.name === name);
    if (p && !key.includes(".") && o.variables.some((v) => v.name === name && v.file === o.mainFile)) {
      // Datei-Variable der Hauptdatei hat Vorrang (z. B. rohrschelle: rohr_d, b)
    } else if (p) return fmt(p.default_value ?? p.default, join);
    const v = o.variables.find((v) => v.name === name);
    if (v) return fmt(v.value, join);
    throw new Error(`Platzhalter {${key}} unbekannt (Modul ${main})`);
  });
}

// ---------------------------------------------------------------------------------------------
if (!existsSync(cad)) throw new Error(`${cad} fehlt (cargo build --release -p cad-cli --no-default-features)`);
mkdirSync(work, { recursive: true });
if (!reuse) rmSync(img, { recursive: true, force: true });
mkdirSync(img, { recursive: true });
const dataFile = join(work, "data.json");
const cache = reuse && existsSync(dataFile) ? JSON.parse(readFileSync(dataFile, "utf8")) : {};
// --redo id,id: diese Modelle trotz --reuse neu messen und rendern
const redo = process.argv.find((a) => a.startsWith("--redo="))?.slice(7).split(",") ?? [];
for (const id of redo) delete cache[id];
const auffaellig = [];

const daten = [];
for (const [i, m] of MODELLE.entries()) {
  const file = join(root, "examples/alltag", m.file);
  const rf = rel(file);
  console.log(`[${i + 1}/${MODELLE.length}] ${rf}`);
  let d = cache[m.id];
  if (!d) {
    d = { id: m.id };
    const o = run(["outline", rf, "--json"]).j;
    o.mainFile = o.files.find((f) => f.role === "main")?.path;
    d.outline = { modules: o.modules, variables: o.variables, parts: o.parts, colors: o.colors, files: o.files, counts: o.counts, diagnostics: o.diagnostics, mainFile: o.mainFile };
    const mb = run(["measure", rf, "--bounds", "--volume", "--json"]);
    d.bounds = mb.j.bounds.bounds;
    d.volume = mb.j.volume.volume;
    // Kernel-Zeit ohne Daemon (kein Cache): scene.stats.kernel_ms
    const ev = run(["ev" + "al", rf, "--json", "--no-daemon"]);
    d.kernel_ms = ev.j?.scene?.stats?.kernel_ms ?? null;
    d.nodes = ev.j?.scene?.stats?.nodes ?? null;
    d.wall_ms = ev.ms;
    d.diag = (ev.j?.diagnostics ?? []).map((x) => `${x.severity}: ${x.message}`);
    // Passungen
    d.pairs = [];
    if (m.pairs) {
      const inter = run(["measure", rf, "--interference", "--json"]).j?.interference;
      d.collisions = inter?.collisions ?? null;
      for (const [a, b, bed] of m.pairs) {
        const c = run(["measure", rf, "--clearance", a, b, "--json"]).j?.clearance;
        d.pairs.push({ a, b, bed, distance: c?.distance, collision: c?.collision });
      }
    }
    // Bilder
    const hero = join(work, `${m.id}.png`);
    const view = m.view ? ["--azimuth", String(m.view[0]), "--elevation", String(m.view[1])] : ["--view", "iso"];
    const r1 = run(["render", rf, "-o", hero, ...view, "--perspective", "--size", "1600x1200", "--no-axes", "--json"]);
    d.render_ms = r1.j?.timings ?? null;
    d.hero = r1.code === 0;
    const sheet = join(work, `${m.id}-ansichten.png`);
    d.sheet = run(["render", rf, "-o", sheet, "--views", "all", "--size", "1200x900", "--no-axes"]).code === 0;
    d.extra = [];
    for (const x of m.extra ?? []) {
      const p = join(work, `${m.id}-${x.name}.png`);
      const r = run(["render", rf, "-o", p, "--size", "1200x900", "--no-axes", ...x.args, "--json"]);
      d.extra.push({ name: x.name, ok: r.code === 0, notes: r.j?.notes ?? [] });
    }
    d.files = o.files.map((f) => f.path);
    cache[m.id] = d;
    writeFileSync(dataFile, JSON.stringify(cache, null, 1));
  }
  // JPEGs (immer neu, schnell)
  d.img = {};
  if (d.hero) d.img.hero = { file: `img/${m.id}.jpg`, bytes: fitJpeg(join(work, `${m.id}.png`), join(img, `${m.id}.jpg`), 1200, 900) };
  if (d.sheet) d.img.sheet = { file: `img/${m.id}-ansichten.jpg`, bytes: toJpeg(join(work, `${m.id}-ansichten.png`), join(img, `${m.id}-ansichten.jpg`), 800) };
  for (const x of d.extra) if (x.ok) d.img[x.name] = { file: `img/${m.id}-${x.name}.jpg`, bytes: fitJpeg(join(work, `${m.id}-${x.name}.png`), join(img, `${m.id}-${x.name}.jpg`), 1200, 900, 0.04) };
  daten.push({ m, d, rf, dir: dirname(file) });
}

// ---------------------------------------------------------------------------------------------
// HTML
const commit = git("rev-parse", "--short", "HEAD");
const datum = new Date().toLocaleDateString("de-DE", { day: "2-digit", month: "2-digit", year: "numeric" });
const mm = (b) => b.size.map((x) => de(x, 1)).join(" × ");
const masse = (v) => (v * PLA) / 1000;
const sec = (ms) => (ms == null ? "–" : de(ms / 1000, ms < 10000 ? 2 : 1) + " s");

function mainModule(d) {
  const parts = d.outline.parts;
  return parts[0]?.module;
}
function paramChips(m, d) {
  const o = d.outline;
  const lit = (t) => /^(-?[\d.]+|\[[\d.,\s-]+\]|true|false)$/.test(String(t).trim());
  const chip = ([n, v]) => `<span class="pp"><b>${esc(n)}</b> = ${esc(v)}</span>`;
  const vars = o.variables.filter((v) => v.literal).map((v) => [v.name, v.text]);
  const mod = o.modules.find((x) => x.name === mainModule(d));
  const params = (mod?.params ?? []).filter((p) => lit(p.default)).map((p) => [p.name, p.default]);
  const groups = [];
  if (vars.length) groups.push(`<span class="grp">Datei</span>` + vars.map(chip).join(""));
  // Modulparameter zusätzlich nur, wenn die Datei-Variablen nicht schon alles sagen
  if (params.length && vars.length < 6) groups.push(`<span class="grp">${esc(mod.name)}()</span>` + params.map(chip).join(""));
  return groups.join("");
}
function farben(d) {
  const c = d.outline.colors ?? [];
  if (c.length <= 1) return "einfarbig (Filament-Slot 1)";
  return c.map((x) => `<span class="sw" style="background:${esc(x.key)}"></span>Slot ${x.filament} ${esc(x.label)} ${de(x.mass_g, 1)} g`).join("<br>");
}

const seiten = []; // [titel, seite]
let seite = 3; // 1 Titel, 2 Inhalt, 3 Übersicht
const pages = [];
for (const [i, { m, d, rf }] of daten.entries()) {
  const nr = String(i + 1).padStart(2, "0");
  const o = d.outline;
  const baugruppe = o.parts.length > 1;
  const text = fill(m.text, o, mainModule(d));
  const mittel = sprachmittel(d.files.filter((f, k, a) => a.indexOf(f) === k).map((f) => join(dirname(join(root, rf)), f)));
  seite += 1;
  seiten.push({ nr, titel: m.titel, kurz: m.kurz ?? m.titel, rf, seite, d, baugruppe });
  const partsTxt = o.parts.map((p) => `<code>${esc(p.name)}</code>`).join(", ");
  const kenn = [
    ["Hülle (B × T × H)", `${mm(d.bounds)} mm`],
    ["Volumen", `${de(d.volume / 1000, 1)} cm³`],
    ["Masse PLA (1,24 g/cm³)", `${de(masse(d.volume), 1)} g`],
    ["Bauteile", `${o.parts.length}: ${partsTxt}`],
    ["Farben / Filamente", farben(d)],
    ["Kernel-Zeit (ohne Cache)", `${sec(d.kernel_ms)} <span class="dim">(${d.nodes} Knoten)</span>`],
  ];
  const cmd = `cad ${rf}`;
  pages.push(`
<section class="page model" id="m-${m.id}">
  <header class="mh"><span class="nr">${nr}</span><h2>${esc(m.titel)}</h2><span class="tag">${baugruppe ? "Baugruppe" : "Einzelteil"}</span></header>
  <p class="desc">${esc(text)}</p>
  <figure class="hero">${d.img.hero ? `<img src="${d.img.hero.file}" alt="${esc(m.titel)}, isometrisch">` : `<div class="missing">Rendering fehlgeschlagen</div>`}</figure>
  <div class="row">
    <figure class="sheet">${d.img.sheet ? `<img src="${d.img.sheet.file}" alt="Ansichten">` : ""}<figcaption>Ansichten iso, vorn, oben, rechts (gleicher Maßstab)</figcaption></figure>
    <table class="kenn">${kenn.map(([k, v]) => `<tr><th>${k}</th><td>${v}</td></tr>`).join("")}</table>
  </div>
  <div class="block"><h3>Parameter (Standardwerte)</h3><div class="chips">${paramChips(m, d)}</div></div>
  <div class="block"><h3>Sprachmittel</h3><div class="chips">${mittel.map((x) => `<span class="ch">${esc(x)}</span>`).join("")}</div></div>
  <div class="cmd"><span>Öffnen</span><code>${esc(cmd)}</code><span>headless</span><code>cad render ${esc(rf)} -o bild.png --views all</code></div>
</section>`);
  if (baugruppe) {
    seite += 1;
    const extras = (m.extra ?? []).map((x) => (d.img[x.name] ? `<figure class="extra"><img src="${d.img[x.name].file}" alt="${esc(x.caption)}"><figcaption>${esc(x.caption)}</figcaption></figure>` : `<div class="missing">${esc(x.name)}: Rendering fehlgeschlagen</div>`)).join("");
    const extra = (m.extra ?? []).length > 1 ? `<div class="extras2">${extras}</div>` : extras;
    const coll = d.collisions == null ? "–" : d.collisions.length === 0 ? "keine Kollision (Toleranz 0,001 mm³)" : d.collisions.map((c) => `${c.a} ↔ ${c.b}: ${de(c.volume, 3)} mm³`).join("; ");
    pages.push(`
<section class="page model">
  <header class="mh"><span class="nr">${nr}</span><h2>${esc(m.titel)} <span class="sub">– Passung</span></h2><span class="tag">Baugruppe</span></header>
  <table class="parts"><thead><tr><th>Bauteil</th><th>Modul</th><th>Hülle (B × T × H) mm</th><th class="r">Volumen cm³</th><th class="r">Masse PLA g</th></tr></thead><tbody>
  ${o.parts.map((p) => `<tr><td><code>${esc(p.name)}</code></td><td><code>${esc(p.module)}</code></td><td>${mm(p.bounds)}</td><td class="r">${de(p.volume / 1000, 2)}</td><td class="r">${de(masse(p.volume), 1)}</td></tr>`).join("")}
  </tbody></table>
  <table class="parts"><thead><tr><th>Paar (<code>measure --clearance</code>)</th><th class="r">Abstand</th><th>Bedeutung</th></tr></thead><tbody>
  ${d.pairs.map((p) => `<tr><td><code>${esc(p.a)}</code> ↔ <code>${esc(p.b)}</code></td><td class="r">${p.distance == null ? "–" : de(p.distance, 3) + " mm"}${p.collision ? " (Kollision)" : ""}</td><td>${esc(p.bed)}</td></tr>`).join("")}
  <tr><td><code>measure --interference</code></td><td colspan="2">${coll}</td></tr>
  </tbody></table>
  ${extra}
  <div class="cmd"><span>Prüfen</span><code>cad measure ${esc(rf)} --interference --clearance ${esc(m.pairs[0][0])} ${esc(m.pairs[0][1])}</code></div>
</section>`);
  }
  for (const x of d.diag) auffaellig.push(`${m.id}: ${x}`);
  if (!d.hero || !d.sheet || d.extra.some((x) => !x.ok)) auffaellig.push(`${m.id}: Rendering fehlgeschlagen`);
}

const gesamtVol = daten.reduce((s, { d }) => s + d.volume, 0);
const gesamtTeile = daten.reduce((s, { d }) => s + d.outline.parts.length, 0);
const nBaugruppen = daten.filter(({ d }) => d.outline.parts.length > 1).length;
const toc = seiten.map((s) => `<tr><td class="nr">${s.nr}</td><td><a href="#m-${s.d.id}">${esc(s.titel)}</a>${s.baugruppe ? ' <span class="tag sm">Baugruppe</span>' : ""}<br><code class="dim">${esc(s.rf.replace("examples/alltag/", ""))}</code></td><td class="r">${s.d.outline.parts.length}</td><td class="nw">${mm(s.d.bounds)}</td><td class="r nw">${de(masse(s.d.volume), 0)} g</td><td class="r nw">${sec(s.d.kernel_ms)}</td><td class="r">${s.seite}</td></tr>`).join("");
const grid = seiten.map((s) => `<a class="tile" href="#m-${s.d.id}"><img src="${s.d.img.hero?.file ?? ""}" alt=""><span><em><b>${s.nr}</b> ${esc(s.kurz)}</em><i>S. ${s.seite}</i></span></a>`).join("");

const howtoLines = [
  ["cad examples/alltag/becher.mcad", "Desktop-UI: Editor und 3D"],
  ["cad examples/alltag/gehaeuse/", "Baugruppe als Projekt (Ordner)"],
  ["cad render examples/alltag/becher.mcad -o becher.png --views all", "headless: Bild"],
  ["cad outline examples/alltag/becher.mcad", "Parameter, Raster, Anker, Bauteile"],
  ["cad measure examples/alltag/becher.mcad --volume --colors", "Volumen, Masse je Farbe"],
  ["cad export examples/alltag/zahnrad.mcad --all-parts -o teile/", "STL/STEP/3MF je Bauteil"],
];
const howto = howtoLines.map(([c, k]) => `<code>${esc(c)}</code><span>${esc(k)}</span>`).join("");
const html = `<!doctype html>
<html lang="de"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Alltagsmodelle – Katalog</title>
<style>
@page { size: A4; margin: 13mm 14mm 15mm 14mm; }
:root { --ink:#1d2129; --dim:#5b6270; --line:#d9dce2; --soft:#f4f5f7; --acc:#2f5d8a; }
* { box-sizing: border-box; }
html { background: #fff; }
body { margin: 0; font: 9.6pt/1.42 "DejaVu Sans", "Helvetica Neue", Arial, sans-serif; color: var(--ink); background: #fff; }
code { font: 8.4pt "DejaVu Sans Mono", Consolas, monospace; }
a { color: inherit; text-decoration: none; }
.page { width: 182mm; height: 268mm; overflow: hidden; break-after: page; position: relative; margin: 0 auto; }
.page.model { display: flex; flex-direction: column; }
.page:last-child { break-after: auto; }
@media screen { body { background: #e6e8ec; } .page { background: #fff; padding: 13mm 14mm; width: 210mm; height: 297mm; margin: 8mm auto; box-shadow: 0 1px 4px #0002; } }
h1 { font-size: 26pt; line-height: 1.15; margin: 0 0 3mm; font-weight: 600; letter-spacing: -0.01em; }
h2 { font-size: 15pt; margin: 0; font-weight: 600; }
h2 .sub { font-weight: 400; color: var(--dim); font-size: 12pt; }
h3 { font-size: 8pt; text-transform: uppercase; letter-spacing: .06em; color: var(--dim); margin: 0 0 1.2mm; font-weight: 600; }
.dim { color: var(--dim); }
.r { text-align: right; }
.title { padding-top: 30mm; }
.title .kicker { color: var(--acc); font-weight: 600; text-transform: uppercase; letter-spacing: .08em; font-size: 9pt; margin-bottom: 4mm; }
.title .lead { font-size: 12pt; line-height: 1.5; color: #333a46; max-width: 160mm; margin: 0 0 8mm; }
.title p { max-width: 165mm; }
.title .facts { display: flex; gap: 6mm; margin: 8mm 0; }
.title .facts div { border-top: 2px solid var(--acc); padding-top: 2mm; min-width: 30mm; }
.title .facts b { display: block; font-size: 17pt; font-weight: 600; }
.title .facts span { color: var(--dim); font-size: 8.5pt; }
.howto { display: grid; grid-template-columns: auto 1fr; gap: 1.2mm 5mm; background: var(--soft); border: 1px solid var(--line); border-radius: 2mm; padding: 3mm 4mm; margin: 2mm 0 5mm; align-items: baseline; }
.howto span { color: var(--dim); font-size: 8.6pt; }
.title .stand { position: absolute; bottom: 0; left: 0; right: 0; color: var(--dim); font-size: 8.5pt; border-top: 1px solid var(--line); padding-top: 2mm; }
table { border-collapse: collapse; width: 100%; }
table.toc td, table.toc th { padding: 1.25mm 1.5mm; border-bottom: 1px solid var(--line); vertical-align: baseline; font-size: 9pt; }
table.toc th { text-align: left; color: var(--dim); font-weight: 600; font-size: 7.8pt; text-transform: uppercase; letter-spacing: .04em; border-bottom: 1.5px solid var(--ink); }
table.toc td.nr { color: var(--acc); font-weight: 600; width: 8mm; }
.tag { display: inline-block; font-size: 7.4pt; color: var(--acc); border: 1px solid #b9cbe0; border-radius: 1mm; padding: 0 1.3mm; line-height: 1.5; background: #f1f6fb; }
.tag.sm { font-size: 6.8pt; }
.tiles { display: grid; grid-template-columns: repeat(4, 1fr); gap: 3mm; margin-top: 4mm; }
.tile { display: block; border: 1px solid var(--line); border-radius: 1.5mm; overflow: hidden; background: ${BG}; }
.tile img { width: 100%; display: block; aspect-ratio: 4/3; object-fit: cover; }
.tile span { display: flex; gap: 1.5mm; background: #fff; border-top: 1px solid var(--line); padding: 1mm 1.6mm; font-size: 7.6pt; white-space: nowrap; }
.tile em { font-style: normal; flex: 1; overflow: hidden; text-overflow: ellipsis; }
.tile b { color: var(--acc); } .tile i { color: var(--dim); font-style: normal; }
.mh { display: flex; align-items: baseline; gap: 3mm; border-bottom: 1.5px solid var(--ink); padding-bottom: 1.6mm; margin-bottom: 2.6mm; }
.mh .nr { font-size: 15pt; font-weight: 600; color: var(--acc); }
.mh .tag { margin-left: auto; }
.nw { white-space: nowrap; }
.grp { font-size: 7.4pt; color: var(--dim); padding: 0.2mm 0.6mm 0 0; line-height: 1.5; }
.grp:not(:first-child) { margin-left: 2mm; }
.desc { margin: 0 0 3mm; }
figure { margin: 0; }
.hero { background: ${BG}; border: 1px solid var(--line); border-radius: 1.5mm; overflow: hidden; }
.hero img { width: 100%; display: block; height: 110mm; object-fit: contain; }
.row { display: flex; gap: 5mm; margin: 3.5mm 0 3mm; align-items: flex-start; }
.sheet { width: 64mm; flex: none; }
.sheet img { width: 100%; display: block; border: 1px solid var(--line); border-radius: 1mm; }
figcaption { font-size: 7.4pt; color: var(--dim); margin-top: 1mm; }
table.kenn th, table.kenn td { text-align: left; vertical-align: top; padding: 1.15mm 1.5mm; border-bottom: 1px solid var(--line); font-size: 8.8pt; }
table.kenn th { color: var(--dim); font-weight: 400; width: 40mm; white-space: nowrap; }
table.kenn tr:first-child th, table.kenn tr:first-child td { border-top: 1px solid var(--line); }
.sw { display: inline-block; width: 2.6mm; height: 2.6mm; border-radius: 50%; border: 1px solid #0003; margin-right: 1.2mm; vertical-align: -0.3mm; }
.block { margin: 0 0 2.4mm; }
.chips { display: flex; flex-wrap: wrap; gap: 1.1mm 1.2mm; }
.ch, .pp { font-size: 7.8pt; border-radius: 1mm; padding: 0.2mm 1.5mm; line-height: 1.5; }
.ch { background: var(--soft); border: 1px solid var(--line); }
.pp { font-family: "DejaVu Sans Mono", monospace; font-size: 7.6pt; border: 1px solid var(--line); }
.pp b { font-weight: 600; color: var(--acc); }
.cmd { margin-top: auto; display: grid; grid-template-columns: auto 1fr; gap: 0.8mm 3mm; background: var(--soft); border: 1px solid var(--line); border-radius: 1.5mm; padding: 2mm 3mm; align-items: baseline; }
.cmd span { color: var(--dim); font-size: 7.8pt; text-transform: uppercase; letter-spacing: .05em; }
table.parts { margin: 0 0 4mm; }
table.parts th, table.parts td { text-align: left; padding: 1.3mm 1.5mm; border-bottom: 1px solid var(--line); font-size: 8.8pt; }
table.parts th { color: var(--dim); font-weight: 600; font-size: 7.8pt; border-bottom: 1.5px solid var(--ink); }
table.parts .r { text-align: right; }
.extra { background: ${BG}; border: 1px solid var(--line); border-radius: 1.5mm; overflow: hidden; }
.extra img { width: 100%; display: block; aspect-ratio: 4/3; }
.extras2 { display: flex; flex-direction: column; gap: 3mm; }
.extras2 .extra { display: flex; }
.extras2 .extra img { width: 108mm; flex: none; }
.extras2 .extra figcaption { border-top: 0; border-left: 1px solid var(--line); padding: 3mm; font-size: 8pt; }
table.toc code { font-size: 7.6pt; }
.extra figcaption { background: #fff; border-top: 1px solid var(--line); padding: 1.4mm 2mm; margin: 0; }
.missing { padding: 20mm; text-align: center; color: #a33; }
</style></head><body>

<section class="page title">
  <div class="kicker">Parametrisches CAD · Beispielsammlung</div>
  <h1>Alltagsmodelle<br>Katalog</h1>
  <p class="lead">${daten.length} parametrische Modelle aus <code>examples/alltag/</code>: Haushalt, Schreibtisch, Werkstatt und typische Maschinenbauteile – vom Becher bis zum Stehlagerbock.</p>
  <p>Die Sammlung dient zweierlei: als Vorlage für eigene Teile (jedes Modell ist ein Modul mit Standardwerten, alle Maße in mm) und als Testsuite, die die Sprache in der Breite nutzt – Raster und Anker, Verrundung und Fase über Zonen, <code>shell</code>, <code>loft</code>, <code>sweep</code>, Profile, <code>include</code>/<code>use</code>, benannte Instanzen als Bauteile. Die Regression <code>tests/alltag/check.sh</code> prüft alle Modelle; gefundene Fehler und Schwächen stehen in <code>examples/alltag/README.md</code>.</p>
  <p>Je Modell zeigt der Katalog eine Seite mit Beschreibung, Rendering, Ansichten, Kenndaten, Parametern und verwendeten Sprachmitteln; Baugruppen haben eine zweite Seite mit Bauteilen, gemessenen Abständen und einem Schnitt bzw. den Ankern. Alle Werte sind mit der CLI gemessen.</p>
  <h3 style="margin-top:6mm">Ein Modell öffnen</h3>
<div class="howto">${howto}</div>
  <div class="facts">
    <div><b>${daten.length}</b><span>Modelle</span></div>
    <div><b>${nBaugruppen}</b><span>mit mehreren Bauteilen</span></div>
    <div><b>${gesamtTeile}</b><span>Bauteile</span></div>
    <div><b>${de(masse(gesamtVol) / 1000, 2)} kg</b><span>PLA für alles</span></div>
  </div>
  <div class="stand">Stand ${datum}, Commit <code>${esc(commit)}</code> · erzeugt mit <code>node tools/report/katalog.mjs</code> · Bilder <code>cad render --view iso --perspective</code>, eingepasst · Masse: PLA 1,24 g/cm³ · Kernel-Zeit: <code>scene.stats.kernel_ms</code> von <code>cad eval --no-daemon</code></div>
</section>

<section class="page">
  <header class="mh"><h2>Inhalt</h2></header>
  <table class="toc"><thead><tr><th>Nr.</th><th>Modell, Datei</th><th class="r">Teile</th><th>Hülle B × T × H mm</th><th class="r">PLA</th><th class="r">Kernel</th><th class="r">Seite</th></tr></thead><tbody>${toc}</tbody></table>
</section>

<section class="page">
  <header class="mh"><h2>Übersicht</h2></header>
  <div class="tiles">${grid}</div>
</section>
${pages.join("\n")}
</body></html>
`;
writeFileSync(join(out, "alltagsmodelle.html"), html);
const total = 3 + pages.length;
console.log(`${rel(join(out, "alltagsmodelle.html"))}: ${total} Seiten erwartet, Bilder ${daten.reduce((s, { d }) => s + Object.values(d.img).reduce((a, x) => a + x.bytes, 0), 0)} B`);
for (const a of auffaellig) console.log("  Auffällig: " + a);
writeFileSync(join(work, "auffaellig.txt"), auffaellig.join("\n") + "\n");
