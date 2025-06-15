#![no_std]
#![feature(allocator_api)]
#![feature(doc_notable_trait)]

pub(crate) mod arch;

pub mod bootstrap;
pub mod context;
pub mod device;
pub mod mm;
pub mod processor;
pub mod trap;

pub use eonix_hal_macros::{ap_main, default_trap_handler, main};
pub use eonix_hal_traits as traits;
