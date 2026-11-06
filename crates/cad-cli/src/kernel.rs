//! The kernel of the commands and of the daemon: solidcore (`kernel-solidcore`), the only one.

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
