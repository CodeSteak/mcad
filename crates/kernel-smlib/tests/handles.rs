//! Handle table leak check, in its own test binary: the facade's live-handle count is
//! process-wide, so tests running in parallel in `kernel.rs` changed it (that test failed
//! once in 100 runs with `--test-threads=8`).

mod common;

use cad_kernel::*;
use common::*;
use glam::DVec3;

#[test]
fn handles_are_released() {
    let mut k = k();
    let before = k.live_handles();
    {
        let b = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
        let c = k.make_cylinder(12.0, 2.0, 2.0, 2).unwrap();
        let c = k.transform(&c, &translate(5.0, 5.0, -1.0)).unwrap();
        let d = k.boolean(BoolOp::Difference, &[b, c], 50).unwrap();
        let _f = k.fillet(&d, &[0], 0.5, 9).unwrap();
        let _c = d.clone();
        assert!(k.live_handles() > before);
    }
    assert_eq!(k.live_handles(), before, "handles leak");
}
