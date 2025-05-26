#![no_std]

#[cfg(target_arch = "x86_64")]
pub use eonix_percpu_macros::define_percpu_x86_64 as define_percpu;

#[cfg(target_arch = "x86_64")]
pub use eonix_percpu_macros::define_percpu_shared_x86_64 as define_percpu_shared;
