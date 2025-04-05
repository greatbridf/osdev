#![no_std]

mod guard;
mod lock;
mod spin;
mod strategy;

pub use guard::Guard;
pub use lock::Lock;
pub use spin::{IrqStrategy, SpinStrategy};
pub use strategy::LockStrategy;

pub type Spin<T> = Lock<T, SpinStrategy>;
