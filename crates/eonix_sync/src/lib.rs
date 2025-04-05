#![no_std]

mod guard;
mod lock;
mod locked;
mod spin;
mod strategy;

pub use guard::Guard;
pub use lock::Lock;
pub use locked::{AsProof, AsProofMut, Locked, Proof, ProofMut};
pub use spin::{IrqStrategy, SpinStrategy};
pub use strategy::LockStrategy;

pub type Spin<T> = Lock<T, SpinStrategy>;
