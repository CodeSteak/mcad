//! The kernel of the commands and of the daemon: solidcore (`kernel-solidcore`), the only one,
//! and the number of threads that evaluate a model (`--jobs`, `MCAD_JOBS`).

use std::sync::{Arc, OnceLock};

use cad_engine::Engine;
use cad_i18n::tr;
use kernel_solidcore::SolidcoreKernel;

/// The kernel type of every command.
pub type CliKernel = SolidcoreKernel;

pub fn new_kernel() -> CliKernel {
    SolidcoreKernel::new()
}

/// Kernel id (part of every cache key; `kernel_id` in the JSON output).
pub fn id() -> &'static str {
    kernel_solidcore::KERNEL_ID
}

/// Threads of this process's evaluations, set once in `main` ([`set_jobs`]).
static JOBS: OnceLock<usize> = OnceLock::new();

/// Largest `--jobs`.
pub const MAX_JOBS: usize = 256;

/// The number of CPUs (`std::thread::available_parallelism`), at least 1.
pub fn default_jobs() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// `--jobs`, else `MCAD_JOBS` (empty: unset), else [`default_jobs`]. Error (usage, exit 2): a
/// `MCAD_JOBS` that is no number from 1 to 256 (clap checks `--jobs`).
pub fn resolve_jobs(flag: Option<u32>) -> Result<usize, String> {
    if let Some(n) = flag {
        return Ok(n as usize);
    }
    match std::env::var("MCAD_JOBS").ok().filter(|v| !v.trim().is_empty()) {
        None => Ok(default_jobs()),
        Some(v) => match v.trim().parse::<usize>() {
            Ok(n) if (1..=MAX_JOBS).contains(&n) => Ok(n),
            _ => Err(tr!(
                "MCAD_JOBS: `{v}` ist keine Anzahl Threads von 1 bis {MAX_JOBS}",
                "MCAD_JOBS: `{v}` is not a number of threads from 1 to {MAX_JOBS}"
            )),
        },
    }
}

/// Sets the threads of this process's evaluations (once, in `main`).
pub fn set_jobs(n: usize) {
    let _ = JOBS.set(n.clamp(1, MAX_JOBS));
}

/// Threads of this process's evaluations ([`set_jobs`], else [`default_jobs`]).
pub fn jobs() -> usize {
    JOBS.get().copied().unwrap_or_else(default_jobs)
}

/// Lets `e` evaluate on `jobs` threads (worker kernels from [`new_kernel`]); nothing if it
/// already does.
pub fn use_jobs(e: &mut Engine<CliKernel>, jobs: usize) {
    if e.jobs() != jobs {
        e.set_workers(jobs, Arc::new(new_kernel));
    }
}
