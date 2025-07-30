#![no_std]
#![feature(allocator_api)]
#![feature(doc_notable_trait)]
#![feature(impl_trait_in_assoc_type)]

pub(crate) mod arch;

pub mod bootstrap;
pub mod context;
pub mod mm;
pub mod trap;

pub mod fence {
    pub use crate::arch::fence::{memory_barrier, read_memory_barrier, write_memory_barrier};
}

pub mod fpu {
    pub use crate::arch::fpu::FpuState;
}

pub mod processor {
    pub use crate::arch::cpu::{halt, UserTLS, CPU, CPU_COUNT};
}

/// Re-export the arch module for use in other crates
///
/// Use this to access architecture-specific functionality in cfg wrapped blocks.
///
/// # Example
/// ``` no_run
/// #[cfg(target_arch = "x86_64")]
/// {
///     use eonix_hal::arch_exported::io::Port8;
///
///     // We know that there will be a `Port8` type available for x86_64.
///     let port = Port8::new(0x3f8);
///     port.write(0x01);
/// }
/// ```
pub mod arch_exported {
    pub use crate::arch::*;
}

pub use eonix_hal_macros::{ap_main, default_trap_handler, main};
pub use eonix_hal_traits as traits;
