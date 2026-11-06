//! Probe of solidcore's ops layer (solidcore-ops, solidcore-tess): the operations mcad needs,
//! each with its expected volume. `cargo run -p kernel-solidcore --features ops-probe --example ops_probe`
use std::f64::consts::PI;
use std::time::Instant;

use solidcore_ops::sweep::Curve;
use solidcore_ops::{self as ops, Blend, Body, BooleanOp, Corners};

fn vol(b: &Body) -> String {
    match b.volume(1e-8) {
        Ok(v) => format!("{v:.6}"),
        Err(e) => format!("volume: {e:?}"),
    }
}

fn report(name: &str, expect: f64, t: Instant, r: Result<Body, ops::Error>) -> Option<Body> {
    let ms = t.elapsed().as_millis();
    match r {
        Ok(b) => {
            let v = b.volume(1e-8).ok();
            let ok = v.is_some_and(|v| (v - expect).abs() <= 1e-6 * expect.abs().max(1.0));
            let c = b.counts().map(|c| format!("{c:?}")).unwrap_or_default();
            println!(
                "{:<5} {name:<34} expect {expect:>12.6} got {:>14} faces/edges/verts {c} {ms} ms",
                if ok { "OK" } else { "DIFF" },
                vol(&b)
            );
            Some(b)
        }
        Err(e) => {
            println!("FAIL  {name:<34} {e:?} {ms} ms");
            None
        }
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Curve> {
    let p = [[x0, y0, 0.0], [x1, y0, 0.0], [x1, y1, 0.0], [x0, y1, 0.0]];
    (0..4).map(|i| Curve::Line { start: p[i], end: p[(i + 1) % 4] }).collect()
}

fn main() {
    let a = 10.0;
    let t = Instant::now();
    report("box 10", 1000.0, t, ops::make_box([0.0; 3], [a; 3]));
    let t = Instant::now();
    report("cylinder r5 h10", PI * 250.0, t, ops::make_cylinder([0.0; 3], 5.0, 10.0));
    let t = Instant::now();
    report("cone r5/r2 h10", PI * 10.0 / 3.0 * (25.0 + 10.0 + 4.0), t, ops::make_cone([0.0; 3], 5.0, 2.0, 10.0));
    let t = Instant::now();
    report("sphere r5", 4.0 / 3.0 * PI * 125.0, t, ops::make_sphere([0.0; 3], 5.0));
    let t = Instant::now();
    report("torus 10/2", 2.0 * PI * PI * 10.0 * 4.0, t, ops::make_torus([0.0; 3], 10.0, 2.0));

    // point classification (inside, outside, on a face)
    if let Ok(b) = ops::make_box([0.0; 3], [1.0; 3]) {
        let c: Vec<_> = [[0.5, 0.5, 0.5], [2.0, 0.5, 0.5], [1.0, 0.5, 0.5]]
            .iter()
            .map(|p| b.classify_point(*p).map(|c| format!("{c:?}")).unwrap_or_else(|e| format!("{e:?}")))
            .collect();
        println!("CLASS unit box inside/outside/on: {c:?}");
    }

    // transform: rotation + translation, non-uniform scale, shear
    let t = Instant::now();
    let r = ops::make_box([0.0; 3], [a; 3]).and_then(|b| {
        let (s, c) = (0.5f64, 0.75f64.sqrt());
        b.transform(&[[c, s, 0.0, 0.0], [-s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [3.0, 4.0, 5.0, 1.0]])?;
        Ok(b)
    });
    report("box rotate 30 + translate", 1000.0, t, r);
    let t = Instant::now();
    let r = ops::make_cylinder([0.0; 3], 5.0, 10.0).and_then(|b| {
        b.transform(&[[2.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]])?;
        Ok(b)
    });
    report("cylinder scale x2", 2.0 * PI * 250.0, t, r);
    let t = Instant::now();
    let r = ops::make_box([0.0; 3], [a; 3]).and_then(|b| {
        b.transform(&[[1.0, 0.0, 0.0, 0.0], [0.5, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]])?;
        Ok(b)
    });
    report("box shear", 1000.0, t, r);

    // booleans
    let t = Instant::now();
    let r = (|| {
        let b = ops::make_box([0.0; 3], [a; 3])?;
        let c = ops::make_cylinder([5.0, 5.0, -1.0], 3.0, 12.0)?;
        ops::boolean::boolean(b, c, BooleanOp::Subtract)?.ok_or(ops::Error::Empty)
    })();
    report("box - cylinder (through hole)", 1000.0 - PI * 9.0 * 10.0, t, r);
    let t = Instant::now();
    let r = (|| {
        let b = ops::make_box([0.0; 3], [a; 3])?;
        let c = ops::make_box([5.0; 3], [a; 3])?;
        ops::boolean::boolean(b, c, BooleanOp::Union)?.ok_or(ops::Error::Empty)
    })();
    report("box + box (overlap)", 2000.0 - 125.0, t, r);
    let t = Instant::now();
    let r = (|| {
        let b = ops::make_box([0.0; 3], [a; 3])?;
        let s = ops::make_sphere([10.0; 3], 5.0)?;
        ops::boolean::boolean(b, s, BooleanOp::Subtract)?.ok_or(ops::Error::Empty)
    })();
    report("box - sphere (corner)", 1000.0 - PI * 125.0 / 6.0, t, r);
    let t = Instant::now();
    let r = (|| {
        let b = ops::make_sphere([0.0; 3], 5.0)?;
        let c = ops::make_cylinder([0.0, 0.0, -6.0], 2.0, 12.0)?;
        ops::boolean::boolean(b, c, BooleanOp::Intersect)?.ok_or(ops::Error::Empty)
    })();
    // sphere ∩ cylinder through the centre: the sphere less the napkin ring, 4π/3 (R³ − h³)
    let h = (25.0f64 - 4.0).sqrt();
    report("sphere ∩ cylinder r2", 4.0 / 3.0 * PI * (125.0 - h.powi(3)), t, r);

    // fillets and chamfers (rounded box: (a-2r)^3 + 6(a-2r)^2 r + 3π(a-2r) r^2 + 4/3 π r^3)
    let rf = 1.0;
    let m = a - 2.0 * rf;
    let rounded = m.powi(3) + 6.0 * m * m * rf + 3.0 * PI * m * rf * rf + 4.0 / 3.0 * PI * rf.powi(3);
    let t = Instant::now();
    let r = (|| {
        let b = ops::make_box([0.0; 3], [a; 3])?;
        let n = b.counts()?[1];
        let edges: Vec<usize> = (0..n).collect();
        ops::blend(&b, Blend::Round, &edges, &[rf], &[], Corners::Filled)?;
        Ok(b)
    })();
    report("fillet all 12 box edges r1", rounded, t, r);
    let t = Instant::now();
    let r = (|| {
        let b = ops::make_box([0.0; 3], [a; 3])?;
        ops::blend(&b, Blend::Round, &[0], &[rf], &[], Corners::Filled)?;
        Ok(b)
    })();
    report("fillet 1 box edge r1", 1000.0 - (1.0 - PI / 4.0) * a, t, r);
    let t = Instant::now();
    let r = (|| {
        let b = ops::make_box([0.0; 3], [a; 3])?;
        ops::blend(&b, Blend::Chamfer, &[0], &[rf], &[], Corners::Filled)?;
        Ok(b)
    })();
    report("chamfer 1 box edge 1", 1000.0 - 0.5 * a, t, r);
    let t = Instant::now();
    let r = (|| {
        let b = ops::make_cylinder([0.0; 3], 5.0, 10.0)?;
        let n = b.counts()?[1];
        // the rims, not the seam (a smooth edge takes no blend)
        let edges: Vec<usize> = (0..n)
            .filter(|&e| {
                b.copy().is_ok_and(|c| ops::blend(&c, Blend::Round, &[e], &[1.0], &[], Corners::Filled).is_ok())
            })
            .collect();
        ops::blend(&b, Blend::Round, &edges, &[1.0], &[], Corners::Filled)?;
        Ok(b)
    })();
    // per rim: torus quarter removed: (1 - π/4)·r²·2π·(R - r) − centroid term (Pappus)
    let (big, r1) = (5.0f64, 1.0f64);
    let area = (1.0 - PI / 4.0) * r1 * r1;
    let cx = big - r1 * (10.0 - 3.0 * PI) / (12.0 - 3.0 * PI);
    report("fillet cylinder rims r1", PI * 250.0 - 2.0 * area * 2.0 * PI * cx, t, r);

    // sketches: extrude, revolve, loft
    let t = Instant::now();
    report(
        "extrude rect 4x6 h5",
        120.0,
        t,
        ops::sweep::extrude(&rect(0.0, 0.0, 4.0, 6.0), [0.0, 0.0, 1.0], 5.0, 0.0, ops::sweep::DraftCorner::Round, true),
    );
    let t = Instant::now();
    report(
        "extrude circle r3 h5",
        PI * 45.0,
        t,
        ops::sweep::extrude(
            &[Curve::Arc {
                center: [0.0; 3],
                axis: [0.0, 0.0, 1.0],
                reference: [1.0, 0.0, 0.0],
                radius: 3.0,
                start: 0.0,
                end: 2.0 * PI,
            }],
            [0.0, 0.0, 1.0],
            5.0,
            0.0,
            ops::sweep::DraftCorner::Round,
            true,
        ),
    );
    let t = Instant::now();
    let prof: Vec<Curve> = {
        let p = [[4.0, 0.0, 0.0], [6.0, 0.0, 0.0], [6.0, 0.0, 3.0], [4.0, 0.0, 3.0]];
        (0..4).map(|i| Curve::Line { start: p[i], end: p[(i + 1) % 4] }).collect()
    };
    report(
        "revolve rect (ring 4..6 h3)",
        PI * (36.0 - 16.0) * 3.0,
        t,
        ops::sweep::revolve(&prof, [0.0; 3], [0.0, 0.0, 1.0], 2.0 * PI, true),
    );
    let t = Instant::now();
    let top: Vec<Curve> = rect(1.0, 1.0, 3.0, 3.0)
        .into_iter()
        .map(|c| match c {
            Curve::Line { start, end } => Curve::Line { start: [start[0], start[1], 5.0], end: [end[0], end[1], 5.0] },
            c => c,
        })
        .collect();
    report(
        "loft 4x4 -> 2x2 h5 (ruled)",
        5.0 / 3.0 * (16.0 + 8.0 + 4.0),
        t,
        ops::sweep::loft(&[rect(0.0, 0.0, 4.0, 4.0), top], 1, true),
    );

    // shell, thicken
    let t = Instant::now();
    let r = (|| {
        let b = ops::make_box([0.0; 3], [a; 3])?;
        let (_, order) = b.to_record()?;
        // every face of the cube gives the same volume when opened
        ops::shell(&b, &order, &[0], -1.0)?;
        Ok(b)
    })();
    report("shell box, one face open, t1", 1000.0 - 8.0 * 8.0 * 9.0, t, r);

    // tessellation
    for (name, b) in [
        ("cylinder", ops::make_cylinder([0.0; 3], 5.0, 10.0)),
        ("sphere", ops::make_sphere([0.0; 3], 5.0)),
        ("torus", ops::make_torus([0.0; 3], 10.0, 2.0)),
    ] {
        let t = Instant::now();
        let r = (|| -> Result<String, String> {
            let b = b.map_err(|e| format!("{e:?}"))?;
            let (rec, _) = b.to_record().map_err(|e| format!("{e:?}"))?;
            let (plan, stats) = solidcore_tess::plan(&rec, &solidcore_tess::MeshTolerance::chord(0.01), 20.0)
                .map_err(|e| format!("{e:?}"))?;
            let (mesh, refused) = plan.evaluate();
            let p = &mesh.positions;
            let mut v = 0.0;
            for tri in &mesh.triangles {
                let [a, b, c] = tri.map(|i| p[i as usize]);
                v += (a.x * (b.y * c.z - b.z * c.y) - a.y * (b.x * c.z - b.z * c.x) + a.z * (b.x * c.y - b.y * c.x))
                    / 6.0;
            }
            Ok(format!(
                "{} triangles, mesh volume {v:.4}, refused faces {}, stats {stats:?}",
                mesh.triangles.len(),
                refused.len()
            ))
        })();
        println!("TESS  {name:<34} {} {} ms", r.unwrap_or_else(|e| format!("FAIL {e}")), t.elapsed().as_millis());
    }
}
