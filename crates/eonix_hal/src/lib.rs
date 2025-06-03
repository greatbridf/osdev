#![no_std]
#![feature(doc_notable_trait)]

pub(crate) mod arch;

pub mod context;
pub mod mm;
pub mod processor;
pub mod trap;

pub use eonix_hal_macros::default_trap_handler;
pub use eonix_hal_traits as traits;
