#![no_std]

mod guard;
mod lazy_lock;
mod locked;
pub mod marker;
mod mutex;
mod rwlock;
mod spin;
mod wait_list;

pub use guard::{ForceUnlockableGuard, UnlockableGuard, UnlockedGuard};
pub use lazy_lock::LazyLock;
pub use locked::{AsProof, AsProofMut, Locked, Proof, ProofMut};
pub use mutex::{Mutex, MutexGuard, Wait as MutexWait};
pub use rwlock::{RwLock, RwLockReadGuard, RwLockWriteGuard, Wait as RwLockWait};
pub use spin::{LoopRelax, Relax, Spin, SpinGuard, SpinRelax, UnlockedSpinGuard};
pub use wait_list::{sleep, yield_now, WaitList};

extern crate alloc;
