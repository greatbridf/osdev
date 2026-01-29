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
    pub use crate::arch::fence::{
        memory_barrier, read_memory_barrier, write_memory_barrier,
    };
}

pub mod fpu {
    pub use crate::arch::fpu::FpuState;
}

pub mod processor {
    pub use crate::arch::cpu::{halt, CPU, CPU_COUNT};
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

#[macro_export]
macro_rules! symbol_addr {
    ($sym:expr) => {{
        ($sym) as *const () as usize
    }};
    ($sym:expr, $type:ty) => {{
        ($sym) as *const () as *const $type
    }};
}

#[macro_export]
macro_rules! extern_symbol_addr {
    ($sym:ident) => {{
        unsafe extern "C" {
            fn $sym();
        }
        $crate::symbol_addr!($sym)
    }};
    ($sym:ident, $type:ty) => {{
        unsafe extern "C" {
            fn $sym();
        }
        $crate::symbol_addr!($sym, $type)
    }};
}

#[macro_export]
macro_rules! extern_symbol_value {
    ($sym:ident) => {{
        unsafe extern "C" {
            fn $sym();
        }

        static SYMBOL_ADDR: &'static unsafe extern "C" fn() =
            &($sym as unsafe extern "C" fn());

        unsafe { (SYMBOL_ADDR as *const _ as *const usize).read_volatile() }
    }};
}
