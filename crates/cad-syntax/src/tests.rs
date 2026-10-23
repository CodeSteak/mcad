use crate::ast::*;
use crate::{LineIndex, parse};

fn ok(src: &str) -> File {
    let (f, d) = parse(src);
    assert!(d.is_empty(), "unexpected diagnostics for {src:?}: {d:?}");
    f
}

#[test]
fn spec_example_gehaeuse() {
    let src = r#"
module gehaeuse(w = 40, d = 30, h = 20, t = 3) {
    grid g {
        x: links = 0, innen_l = t, innen_r = w - t, rechts = w;
        y: vorne = 0, hinten = d;
        z: boden = 0, deckel = h;
    }

    difference() {
        box(g[links, vorne, boden], g[rechts, hinten, deckel]);
        box(g[innen_l, vorne, t], g[innen_r, hinten, deckel]);
    }

    anchor ecke = g[rechts, hinten, deckel];
    anchor oben = g[rechts, *, deckel], dir = [0, 0, 1];
}

k: gehaeuse(w = 60);
translate(k.ecke) rotate(to = dir(k.ecke)) deckel();
grid h { x: rand = k.ecke.x; }
"#;
    let f = ok(src);
    assert_eq!(f.stmts.len(), 4);
    let StmtKind::Module(m) = &f.stmts[0].kind else { panic!() };
    assert_eq!(m.name.name, "gehaeuse");
    assert_eq!(m.params.len(), 4);
    let StmtKind::Grid(g) = &m.body[0].kind else { panic!("{:?}", m.body[0].kind) };
    assert_eq!(g.planes.len(), 8);
    assert_eq!(g.planes[2].name.name, "innen_r");
    assert!(matches!(m.body[2].kind, StmtKind::Anchor { .. }));
    let StmtKind::Anchor { dir, point, .. } = &m.body[3].kind else { panic!() };
    assert!(dir.is_some());
    let ExprKind::Index { items, .. } = &point.kind else { panic!() };
    assert!(matches!(items[1], IndexItem::Wild(_)));
    assert!(matches!(f.stmts[1].kind, StmtKind::Named { .. }));
}

#[test]
fn transforms_before_grid() {
    let f = ok("translate([1,2,3]) rotate([0, 0, 90]) grid r { x: a = 0; }");
    let StmtKind::Call { name, children, .. } = &f.stmts[0].kind else { panic!() };
    assert_eq!(name.name, "translate");
    let StmtKind::Call { children: c2, .. } = &children[0].kind else { panic!() };
    assert!(matches!(c2[0].kind, StmtKind::Grid(_)));
}

#[test]
fn grid_as_module_name_is_still_a_call() {
    let f = ok("grid(3); module grid(n) { cube(n); }");
    assert!(matches!(f.stmts[0].kind, StmtKind::Call { .. }));
}

#[test]
fn expressions_and_precedence() {
    let f = ok("a = 1 + 2 * 3 ^ 2 - -4 % 3 == 5 && !b || c ? x : y;");
    let StmtKind::Assign { value, .. } = &f.stmts[0].kind else { panic!() };
    assert!(matches!(value.kind, ExprKind::Ternary(..)));
}

#[test]
fn list_comprehensions() {
    ok("xs = [for (i = [0:4]) i * 8];");
    ok("xs = [for (i = [0:2:10]) if (i > 2) i else -i];");
    ok("xs = [for (i = 0; i < 5; i = i + 1) let (j = i * 2) j];");
    ok("xs = [each [1, 2], for (a = [1, 2], b = [3]) [a, b]];");
    ok("xs = [1, 2, 3,];");
    ok("e = [];");
}

#[test]
fn function_literals_let_assert_echo() {
    ok("f = function (x) x * 2; y = f(3);");
    ok("function g(a, b = 2) = let (c = a + b) c * c;");
    ok("v = assert(true, \"ok\") 3;");
    ok("v = echo(\"hi\", 2) 3;");
    ok("fillet(r = function (p) p.z < 10 ? 2 : 4) { cube(10); }");
}

#[test]
fn modifiers_and_control() {
    let f = ok("#cube(1); %sphere(2); !cylinder(h = 1, r = 2); *cube(3); \
                for (i = [0:3]) translate([i, 0, 0]) cube(1); \
                if (a) cube(1); else { sphere(1); } \
                intersection_for (i = [1, 2]) cube(i);");
    assert_eq!(f.stmts[0].modifiers, vec![Modifier::Highlight]);
    assert_eq!(f.stmts[3].modifiers, vec![Modifier::Disable]);
}

#[test]
fn include_and_use() {
    let f = ok("include <foo/bar.scad>\nuse <lib.scad>;\ncube(1);");
    assert!(matches!(&f.stmts[0].kind, StmtKind::Include { path, is_use: false, .. } if path == "foo/bar.scad"));
    assert!(matches!(&f.stmts[1].kind, StmtKind::Include { is_use: true, .. }));
}

#[test]
fn comments_and_numbers() {
    let f = ok("/* block */ a = .5; // line\nb = 1e-3; c = 2.; d = 1.5E+2;");
    let StmtKind::Assign { value, .. } = &f.stmts[3].kind else { panic!() };
    assert_eq!(value.kind, ExprKind::Number(150.0));
}

#[test]
fn literal_spans_cover_the_text() {
    let src = "translate([20, 0, 10]) cube(5);";
    let f = ok(src);
    let StmtKind::Call { args, .. } = &f.stmts[0].kind else { panic!() };
    let ExprKind::Vector(el) = &args[0].value.kind else { panic!() };
    let VecElem::Expr(e) = &el[0] else { panic!() };
    assert_eq!(&src[e.span.start as usize..e.span.end as usize], "20");
    assert_eq!(&src[args[0].value.span.start as usize..args[0].value.span.end as usize], "[20, 0, 10]");
}

#[test]
fn error_recovery_keeps_following_statements() {
    let (f, d) = parse("a = ;\ncube(1);\nsphere(;\ncylinder(h = 2, r = 1);");
    assert_eq!(d.len(), 2, "{d:?}");
    let calls = f.stmts.iter().filter(|s| matches!(s.kind, StmtKind::Call { .. })).count();
    assert_eq!(calls, 2);
}

#[test]
fn unclosed_block_reports_error() {
    let (_f, d) = parse("difference() { cube(1);");
    assert!(!d.is_empty());
}

#[test]
fn stmt_ids_are_unique() {
    let f = ok("module m() { cube(1); sphere(1); } m(); translate([1,0,0]) m();");
    let mut ids = Vec::new();
    for s in &f.stmts {
        s.walk(&mut |s| ids.push(s.id));
    }
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n);
}

#[test]
fn identifiers_may_start_with_digits() {
    // OpenSCAD's lexer takes the longest match: `2d` and `0x1F` are identifiers,
    // `1e5` and `12` stay numbers.
    use crate::lexer::Tok;
    let mut d = Vec::new();
    let toks: Vec<Tok> = crate::lexer::lex("2d 3x_1 0x1F 1e5 12 1.5 1e2x", &mut d).into_iter().map(|t| t.tok).collect();
    assert!(d.is_empty(), "{d:?}");
    let id = |s: &str| Tok::Ident(s.into());
    let want = [id("2d"), id("3x_1"), id("0x1F"), Tok::Number(1e5), Tok::Number(12.0), Tok::Number(1.5), id("1e2x")];
    assert_eq!(&toks[..7], &want);
    ok("module 2d_profile() square(1); 2d_profile();");
}

#[test]
fn messages_are_german() {
    let (_, d) = parse("cube(1;");
    assert!(d[0].message.contains("erwartet, gefunden"), "{d:?}");
    let (_, d) = parse("a = \"x;");
    assert!(d.iter().any(|d| d.message == "String nicht geschlossen"), "{d:?}");
}

#[test]
fn line_index_utf16() {
    let src = "a = 1; // äö😀\nb = 2;";
    let li = LineIndex::new(src);
    let b_byte = src.find('b').unwrap() as u32;
    let b_u16 = src.encode_utf16().count() as u32 - 6;
    assert_eq!(li.byte_to_utf16(b_byte), b_u16);
    assert_eq!(li.utf16_to_byte(b_u16), b_byte);
    assert_eq!(li.line_of(b_byte), 1);
    assert_eq!(li.line_col(b_byte + 2), (1, 2));
}

#[test]
fn indexed_named_instance() {
    let f = ok("for (i = [0:3]) k[i]: halter();\nk[2 * 1]: halter();");
    let StmtKind::For { body, .. } = &f.stmts[0].kind else { panic!() };
    let StmtKind::Named { name, index: Some(ix), inst } = &body.kind else { panic!("{:?}", body.kind) };
    assert_eq!(name.name, "k");
    assert_eq!(ix.kind, ExprKind::Ident("i".into()));
    assert!(matches!(inst.kind, StmtKind::Call { .. }));
    assert!(matches!(f.stmts[1].kind, StmtKind::Named { index: Some(_), .. }));
}

#[test]
fn file_regions() {
    let (f, d) = crate::parse_file("a = 1;\ncube(a);", 3);
    assert!(d.is_empty());
    let base = 3 << crate::FILE_SHIFT;
    assert_eq!(f.stmts[0].span.start, base);
    assert_eq!(f.stmts[0].span.file(), 3);
    assert_eq!(f.stmts[1].span.local(), crate::Span::new(7, 15));
    assert!(f.stmts.iter().all(|s| s.id >= 3 << crate::ID_SHIFT));
    let (_, d) = crate::parse_file("cube(;", 2);
    assert_eq!(d[0].span.file(), 2);
}

#[test]
fn include_path_span() {
    let src = "include <a/b.scad>\nuse <c.scad>;";
    let f = ok(src);
    let StmtKind::Include { path_span, .. } = &f.stmts[0].kind else { panic!() };
    assert_eq!(&src[path_span.start as usize..path_span.end as usize], "<a/b.scad>");
}
