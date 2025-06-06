#![no_std]

mod guard;
mod lazy_lock;
mod locked;
mod marker;
mod relax;

pub use guard::{UnlockableGuard, UnlockedGuard};
pub use lazy_lock::LazyLock;
pub use locked::{AsProof, AsProofMut, Locked, Proof, ProofMut};
pub use marker::{NotSend, NotSync};
pub use relax::{LoopRelax, Relax, SpinRelax};
