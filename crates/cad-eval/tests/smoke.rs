mod common;
use common::*;

#[test]
fn smoke_gehaeuse() {
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
}
k: gehaeuse(w = 60);
translate(k.ecke) cube(5);
echo(k.ecke);
"#;
    let out = run(src);
    println!("{}", dump(&out));
    println!("{:#?}", out.diagnostics);
    println!("{:?}", echoes(&out));
    println!("handles {}", out.handles.len());
    for h in &out.handles {
        println!("{:?} {:?} {:?} via_var={} comps={:?}", h.kind, h.pos, h.value, h.via_var, h.comps);
    }
}
