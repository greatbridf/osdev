#![no_std]

mod guard;
mod lazy_lock;
mod locked;
pub mod marker;
mod mutex;
mod rwlock;
mod spin;
mod wait_list;

pub use guard::{UnlockableGuard, UnlockedGuard};
pub use lazy_lock::LazyLock;
pub use locked::{AsProof, AsProofMut, Locked, Proof, ProofMut};
pub use mutex::{Mutex, MutexGuard};
pub use rwlock::{RwLock, RwLockReadGuard, RwLockWriteGuard};
pub use spin::{
    ContextUnlock, DisablePreemption, LoopRelax, NoContext, Relax, Spin, SpinContext, SpinGuard,
    SpinIrq, SpinRelax, UnlockedContext, UnlockedSpinGuard,
};
pub use wait_list::WaitList;

extern crate alloc;
