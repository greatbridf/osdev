#![no_std]

mod guard;
mod lock;
mod locked;
mod rwlock;
mod spin;
mod strategy;

pub use guard::Guard;
pub use lock::Lock;
pub use locked::{AsProof, AsProofMut, Locked, Proof, ProofMut};
pub use rwlock::RwLockStrategy;
pub use spin::{IrqStrategy, SpinStrategy};
pub use strategy::{LockStrategy, WaitStrategy};

pub type Spin<T> = Lock<T, SpinStrategy>;
pub type RwLock<T, W> = Lock<T, RwLockStrategy<W>>;
