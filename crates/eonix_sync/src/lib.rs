#![no_std]

mod guard;
mod lock;
mod locked;
mod marker;
mod spin;
mod strategy;

pub use guard::{ForceUnlockableGuard, Guard, UnlockableGuard, UnlockedGuard};
pub use lock::Lock;
pub use locked::{AsProof, AsProofMut, Locked, Proof, ProofMut};
pub use spin::{LoopRelax, Relax, Spin, SpinGuard, SpinIrqGuard, SpinRelax};
pub use strategy::LockStrategy;
