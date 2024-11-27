#![no_std]

mod arch;

pub use arch::set_percpu_area_thiscpu;
pub use percpu_macros::define_percpu;
