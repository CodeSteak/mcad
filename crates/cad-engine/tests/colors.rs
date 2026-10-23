//! Colour regions, filament slots and multi-colour exports (Plan 16) with the native SMLib
//! kernel: source → `cad_eval` → engine.

mod common;

use std::f64::consts::PI;
use std::io::Read;

use cad_engine::colors::{ColorSource, ColorTable, RegionColor};
use cad_eval::palette::{self, DEFAULT_RGB, palette_color, rgba_of};
use cad_eval::{EvalOptions, eval_source};
use common::*;

fn tree(src: &str) -> cad_ir::Tree {
    let out = eval_source(src, &EvalOptions::default());
    let errs: Vec<_> = out.errors().collect();
    assert!(errs.is_empty(), "{errs:#?}");
    out.tree
}

fn table(t: &cad_ir::Tree) -> ColorTable {
    ColorTable::of(t, rgba_of(DEFAULT_RGB), &|n| palette_color(n).is_some())
}

fn key(name: &str) -> [u8; 4] {
    let c = palette_color(name).unwrap().rgb;
    [c[0], c[1], c[2], 255]
}

/// (label, filament, volume) of the root's regions.
fn regions(e: &mut E, ev: &Ev, t: &ColorTable) -> Vec<(String, u32, f64)> {
    let r = e.color_regions(ev, ev.root().unwrap()).expect("regions");
    let sum: f64 = r.regions.iter().map(|x| x.volume).sum();
    assert_approx("sum of region volumes", sum, r.total, 1e-6);
    assert_approx("total = root volume", r.total, root_volume(e, ev), 1e-9);
    r.regions
        .iter()
        .map(|x| {
            let en = &t.entries[t.entry_of(x.color).expect("entry")];
            (en.label.clone(), en.filament, x.volume)
        })
        .collect()
}

#[track_caller]
fn check(got: &[(String, u32, f64)], want: &[(&str, u32, f64)]) {
    assert_eq!(got.len(), want.len(), "{got:?}");
    for (g, w) in got.iter().zip(want) {
        assert_eq!((g.0.as_str(), g.1), (w.0, w.1), "{got:?}");
        assert_approx(w.0, g.2, w.2, 1e-6);
    }
}

#[test]
fn later_statement_wins_overlaps() {
    let mut e = engine();
    let t = tree("color(\"red\") cube(10);\ncolor(\"blue\") translate([5, 0, 0]) cube(10);");
    let ct = table(&t);
    let ev = e.evaluate(t);
    assert_clean(&ev);
    check(&regions(&mut e, &ev, &ct), &[("red", 1, 500.0), ("blue", 2, 1000.0)]);
}

#[test]
fn nested_colour_wins_over_its_parent() {
    let mut e = engine();
    let t = tree("color(\"red\") { cube(10); color(\"white\") translate([2, 2, 8]) cube([6, 6, 4]); }");
    let ct = table(&t);
    let ev = e.evaluate(t);
    check(&regions(&mut e, &ev, &ct), &[("red", 1, 1000.0 - 72.0), ("white", 2, 144.0)]);
}

#[test]
fn subtracted_material_counts_for_all_colours() {
    let mut e = engine();
    let t = tree(
        "difference() {\n  union() { color(\"red\") cube(10); color(\"blue\") translate([0, 0, 10]) cube(10); }\n  \
         color(\"green\") translate([5, 5, -1]) cylinder(h = 30, r = 2);\n}",
    );
    let ct = table(&t);
    // green is only in the subtracted operand: no material, no slot
    assert_eq!(ct.entries.iter().map(|e| e.label.as_str()).collect::<Vec<_>>(), ["red", "blue"]);
    let ev = e.evaluate(t);
    let hole = PI * 4.0 * 10.0;
    let got = regions(&mut e, &ev, &ct);
    assert_eq!(got.len(), 2);
    assert_approx("red", got[0].2, 1000.0 - hole, 1e-3);
    assert_approx("blue", got[1].2, 1000.0 - hole, 1e-3);
}

#[test]
fn uncoloured_material_is_the_default_colour_on_filament_1() {
    let mut e = engine();
    let t = tree("cube(10);\ncolor(\"red\") translate([0, 0, 10]) cube(2);");
    let ct = table(&t);
    assert_eq!(ct.entries[0].source, ColorSource::Default);
    assert_eq!(ct.entries[0].label, "Standard");
    let ev = e.evaluate(t);
    check(&regions(&mut e, &ev, &ct), &[("Standard", 1, 1000.0), ("red", 2, 8.0)]);
}

#[test]
fn filament_slots() {
    // explicit slots stay, others take the lowest free slot in order of first appearance
    let t = tree(
        "color(\"teal\", filament = 3) cube(1);\ntranslate([2, 0, 0]) color(\"red\") cube(1);\n\
         translate([4, 0, 0]) color(\"blue\") cube(1);\ntranslate([6, 0, 0]) color(\"teal\") cube(1);\n\
         translate([8, 0, 0]) color(\"#123456\") cube(1);\ntranslate([10, 0, 0]) color(\"bluish\") cube(1);",
    );
    let ct = table(&t);
    let got: Vec<(&str, u32, bool, ColorSource)> =
        ct.entries.iter().map(|e| (e.label.as_str(), e.filament, e.explicit, e.source)).collect();
    assert_eq!(
        got,
        [
            ("teal", 3, true, ColorSource::Palette),
            ("red", 1, false, ColorSource::Palette),
            ("blue", 2, false, ColorSource::Palette),
            ("#123456", 4, false, ColorSource::Exact),
            ("bluish", 5, false, ColorSource::Random),
        ]
    );
    assert_eq!(ct.entries[0].nodes.len(), 2, "both teal statements");
    assert!(ct.entries[0].explicit_node.is_some());
    // with uncoloured material: default 1, the rest shifts
    let t = tree("cube(1);\ncolor(\"red\") cube(1);\ncolor(\"blue\", filament = 2) cube(1);\ncolor(\"lime\") cube(1);");
    let ct = table(&t);
    let got: Vec<(&str, u32)> = ct.entries.iter().map(|e| (e.label.as_str(), e.filament)).collect();
    assert_eq!(got, [("Standard", 1), ("red", 3), ("blue", 2), ("lime", 4)]);
    // equal colours in different spellings are one entry
    let t = tree("color(\"gray\") cube(1); color(\"GREY\") cube(1); color(\"#7b7d81\") cube(1);");
    assert_eq!(table(&t).entries.len(), 1);
}

#[test]
fn fillet_between_colours_goes_to_the_first_coloured_subtree() {
    let mut e = engine();
    let t = tree(
        "union(fillet = 1) {\n  color(\"red\") cube(10);\n  color(\"blue\") translate([3, 3, 9]) cube([4, 4, 5]);\n}",
    );
    let ct = table(&t);
    let ev = e.evaluate(t);
    assert_clean(&ev);
    let got = regions(&mut e, &ev, &ct);
    assert_eq!(got.iter().map(|g| g.0.as_str()).collect::<Vec<_>>(), ["red", "blue"]);
    assert_approx("blue (box only)", got[1].2, 80.0, 1e-6);
    assert!(got[0].2 > 1000.0 - 16.0 + 1.0, "red gets the fillet material: {}", got[0].2);
}

#[test]
fn resize_scales_the_colour_material() {
    let mut e = engine();
    let t = tree(
        "resize([40, 20, 20]) {\n  color(\"red\") cube(10);\n  color(\"blue\") translate([10, 0, 0]) cube(10);\n}",
    );
    let ct = table(&t);
    let ev = e.evaluate(t);
    check(&regions(&mut e, &ev, &ct), &[("red", 1, 8000.0), ("blue", 2, 8000.0)]);
}

#[test]
fn transforms_and_modules() {
    let mut e = engine();
    let t = tree(
        "module knopf(c) color(c) cylinder(h = 2, r = 3);\n\
         color(\"black\") cube([30, 10, 2]);\n\
         for (i = [0:2]) translate([5 + 10 * i, 5, 2]) rotate([0, 0, 30]) knopf(i == 1 ? \"red\" : \"white\");",
    );
    let ct = table(&t);
    let ev = e.evaluate(t);
    let knob = PI * 9.0 * 2.0;
    let got = regions(&mut e, &ev, &ct);
    let labels: Vec<&str> = got.iter().map(|g| g.0.as_str()).collect();
    assert_eq!(labels, ["black", "white", "red"]);
    assert_approx("black", got[0].2, 600.0, 1e-9);
    assert_approx("white", got[1].2, 2.0 * knob, 1e-3);
    assert_approx("red", got[2].2, knob, 1e-3);
}

#[test]
fn regions_are_cached_and_colour_changes_recompute_no_nodes() {
    let mut e = engine();
    let src = |c: &str| format!("color(\"{c}\") cube(10);\ncolor(\"blue\") translate([5, 0, 0]) cube(10);");
    let ev = e.evaluate(tree(&src("red")));
    let root = ev.root().unwrap();
    let a = e.color_regions(&ev, root).unwrap();
    let calls = e.kernel().total();
    let b = e.color_regions(&ev, root).unwrap();
    assert!(std::rc::Rc::ptr_eq(&a, &b));
    assert_eq!(e.kernel().total(), calls, "cached regions need no kernel call");
    // another colour or filament slot: every node result comes from the cache
    let ev2 = e.evaluate(tree(&src("green")));
    assert_eq!(ev2.stats.cache_hits, ev2.stats.nodes, "colour is not part of node keys");
    let c = e.color_regions(&ev2, ev2.root().unwrap()).unwrap();
    assert!(!std::rc::Rc::ptr_eq(&a, &c), "regions depend on the colours");
    assert_eq!(c.regions[0].color, RegionColor::Color(key("green")));
    let ev3 =
        e.evaluate(tree("color(\"green\", filament = 4) cube(10);\ncolor(\"blue\") translate([5, 0, 0]) cube(10);"));
    assert_eq!(ev3.stats.cache_hits, ev3.stats.nodes);
    let d = e.color_regions(&ev3, ev3.root().unwrap()).unwrap();
    assert!(std::rc::Rc::ptr_eq(&c, &d), "the slot does not change the regions");
}

fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            (f.name().to_string(), b)
        })
        .collect()
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let p = format!(" {name}=\"");
    let i = tag.find(&p)? + p.len();
    Some(&tag[i..i + tag[i..].find('"')?])
}

#[test]
fn multicolour_3mf_structure() {
    let mut e = engine();
    let out = eval_source(
        "module deckel() color(\"white\") { cube([20, 20, 2]); color(\"red\", filament = 3) translate([5, 5, 2]) cube([10, 10, 1]); }\n\
         module boden() translate([30, 0, 0]) color(\"teal\") cube([20, 20, 5]);\n\
         deckel: deckel();\nboden: boden();",
        &EvalOptions::default(),
    );
    assert!(out.errors().next().is_none());
    let named: Vec<(String, u32)> = out.parts.iter().map(|p| (p.name.clone(), p.node)).collect();
    assert_eq!(named.len(), 2, "{named:?}");
    let ct = table(&out.tree);
    let ev = e.evaluate(out.tree);
    let (bytes, st) = e.export_3mf_colors(&ev, &named, None, &ct, "controller-cad test").unwrap();
    assert_eq!((st.objects, st.parts), (2, 3));
    let files = unzip(&bytes);
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["[Content_Types].xml", "_rels/.rels", "3D/3dmodel.model", "Metadata/model_settings.config"]);
    let model = String::from_utf8(files[2].1.clone()).unwrap();
    let cfg = String::from_utf8(files[3].1.clone()).unwrap();
    // colour group: white, red, teal in order of appearance
    let colors: Vec<&str> =
        model.match_indices("<m:color ").map(|(i, _)| attr(&model[i..], "color").unwrap()).collect();
    assert_eq!(colors, ["#F4F3F0FF", "#C74240FF", "#217274FF"]);
    // mesh objects carry pid/pindex, part objects only components without transforms
    let objs: Vec<&str> =
        model.match_indices("<object ").map(|(i, _)| &model[i..i + model[i..].find('>').unwrap()]).collect();
    assert_eq!(objs.len(), 5);
    let meshes: Vec<_> = objs.iter().filter(|o| o.contains("pid=")).collect();
    assert_eq!(meshes.len(), 3);
    assert_eq!(model.matches("<component ").count(), 3);
    assert!(!model.contains("transform="));
    assert_eq!(model.matches("<item ").count(), 2);
    // model_settings: part ids = mesh object ids, extruder = slot
    let white_slot = ct.entries[ct.find(key("white")).unwrap()].filament;
    let teal_slot = ct.entries[ct.find(key("teal")).unwrap()].filament;
    assert_eq!((white_slot, teal_slot), (1, 2));
    for (o, slot) in [("deckel – white", 1), ("deckel – red", 3), ("boden – teal", 2)] {
        let i = cfg.find(&format!("value=\"{o}\"")).unwrap_or_else(|| panic!("{o} in {cfg}"));
        let ext = &cfg[i..];
        let ext = attr(&ext[ext.find("key=\"extruder\"").unwrap() - 1..], "value").unwrap();
        assert_eq!(ext, slot.to_string(), "{o}");
        let mesh_obj = objs.iter().find(|x| x.contains(&format!("name=\"{o}\""))).unwrap();
        let part_tag = &cfg[..i];
        let part_tag = &part_tag[part_tag.rfind("<part ").unwrap()..];
        assert_eq!(attr(part_tag, "id"), attr(mesh_obj, "id"), "{o}");
    }
    assert_eq!(cfg.matches("<object ").count(), 2);
    // volumes per colour
    let red = st.colors.iter().find(|c| c.0 == "red").unwrap();
    assert_eq!(red.1, 3);
    assert_approx("red", red.2, 100.0, 1e-6);

    // single colour export of the same evaluation still works
    let (single, _) = e.export_nodes(&ev, &named, cad_engine::NodeFormat::ThreeMf, None).unwrap();
    assert!(!String::from_utf8_lossy(&unzip(&single)[2].1).contains("m:colorgroup"));

    // one STL per colour
    let (zip, _) = e.export_color_stl_zip(&ev, &named, None, &ct).unwrap();
    let names: Vec<String> = unzip(&zip).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["deckel_F1_white.stl", "deckel_F3_red.stl", "boden_F2_teal.stl"]);
}

#[test]
fn region_meshes_share_coordinates() {
    let mut e = engine();
    let t = tree("color(\"red\") cube(10);\ncolor(\"blue\") translate([5, 0, 0]) cube(10);");
    let ct = table(&t);
    let ev = e.evaluate(t);
    let root = ev.root().unwrap();
    let r = e.color_regions(&ev, root).unwrap();
    let mut vol = 0.0;
    for i in 0..r.regions.len() {
        let m = e.region_mesh(&r, i, 0.01).unwrap();
        vol += mesh_volume(&m);
    }
    assert_approx("mesh volumes", vol, 1500.0, 1e-4);
    let (objs, colors) = e.color_objects(&ev, &[("m".into(), root)], None, &ct).unwrap();
    let b0 = objs[0].parts[0].mesh.bounds().unwrap();
    let b1 = objs[0].parts[1].mesh.bounds().unwrap();
    assert_approx("red x0", b0.0.x, 0.0, 1e-9);
    assert_approx("blue x0", b1.0.x, 5.0, 1e-6);
    assert_eq!(colors.iter().map(|c| c.filament).collect::<Vec<_>>(), [1, 2]);
    let _ = palette::PALETTE;
}

#[test]
fn uncoloured_part_needs_no_boolean() {
    // No colour below the root: one Standard region from the root's solids as they are (no
    // Boolean), volume = the cached part volume (`Engine::mass_props`); the mesh holds both cubes.
    let mut e = engine();
    let t = tree("cube(10);\ntranslate([20, 0, 0]) cube(5);");
    let ct = table(&t);
    let ev = e.evaluate(t);
    assert_clean(&ev);
    check(&regions(&mut e, &ev, &ct), &[("Standard", 1, 1125.0)]);
    let r = e.color_regions(&ev, ev.root().unwrap()).unwrap();
    assert!(!r.regions[0].shapes.is_empty());
    let m = e.region_mesh(&r, 0, 0.05).unwrap();
    let xmax = m.positions.chunks(3).map(|p| p[0]).fold(f32::MIN, f32::max);
    assert!((xmax - 25.0).abs() < 1e-4, "{xmax}");
}
