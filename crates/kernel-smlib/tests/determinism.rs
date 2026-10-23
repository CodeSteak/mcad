//! Determinism of the facade (own test binary: the test calls the C library's rand()
//! directly, outside the kernel lock, so no other test may run kernel operations meanwhile).

mod common;

use cad_kernel::*;
use common::*;
use glam::DVec3;

/// SMLib seeds rand() with srand(time(NULL)) inside operations (SmFace::GetPointsInFace,
/// SmShell::IsInnerShellOf). An operation run in another second, or spanning a second
/// boundary, saw other sample points: `kernel.rs::invalid_profiles_fail_cleanly` (too-thick
/// shell must fail) succeeded in 4 of 40 runs with `--test-threads=8`, never with a fixed
/// time. The facade defines rand()/srand() itself: srand() restarts a fixed sequence and so
/// does every facade call. (The remaining 1 in 100 came from a use-after-free in SMLib's
/// shell; the facade now rejects such walls before calling it, see `sc_shell`.)
#[test]
fn rand_is_deterministic() {
    unsafe extern "C" {
        fn rand() -> std::ffi::c_int;
        fn srand(seed: std::ffi::c_uint);
    }
    let mut k = k();
    let draw = |seed| unsafe {
        srand(seed);
        (0..4).map(|_| rand()).collect::<Vec<_>>()
    };
    let a = draw(1);
    assert_eq!(a, draw(1_792_700_000), "srand() must not depend on its seed (time)");
    assert!(a.iter().all(|&v| v >= 0));
    assert!(a.windows(2).any(|w| w[0] != w[1]));
    // the same operation gives the same result, whatever ran before
    let b = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    for i in 0..5 {
        unsafe {
            srand(i);
            rand();
        }
        assert!(matches!(k.shell(&b, 6.0, &[0], 3), Err(KernelError::Failed { .. })));
        let s = k.shell(&b, 1.0, &[0], 3).unwrap();
        assert_approx("shell", volume(&mut k, &s), 1000.0 - 8.0 * 8.0 * 9.0, 1e-6);
    }
}
