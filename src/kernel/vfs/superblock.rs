use core::{
    marker::Unsize,
    ops::{CoerceUnsized, Deref},
};

use alloc::sync::{Arc, Weak};
use eonix_sync::RwLock;

use crate::{kernel::constants::EIO, prelude::KResult};

use super::types::DeviceId;

pub trait SuperBlock: Send + Sync + 'static {}

#[derive(Debug, Clone)]
pub struct SuperBlockInfo {
    pub io_blksize: u32,
    pub device_id: DeviceId,
    pub read_only: bool,
}

pub struct SuperBlockLock(());

pub struct SuperBlockComplex<Backend>
where
    Backend: SuperBlock + ?Sized,
{
    pub info: SuperBlockInfo,
    pub rwsem: RwLock<SuperBlockLock>,
    pub backend: Backend,
}

pub struct SbRef<S>(Weak<SuperBlockComplex<S>>)
where
    S: SuperBlock + ?Sized;

pub struct SbUse<S>(Arc<SuperBlockComplex<S>>)
where
    S: SuperBlock + ?Sized;

impl<S> SbRef<S>
where
    S: SuperBlock + ?Sized,
{
    pub fn try_get(&self) -> Option<SbUse<S>> {
        self.0.upgrade().map(|arc| SbUse(arc))
    }

    pub fn get(&self) -> KResult<SbUse<S>> {
        self.try_get().ok_or(EIO)
    }

    pub fn from(sb: &SbUse<S>) -> Self {
        SbRef(Arc::downgrade(&sb.0))
    }

    pub fn eq<U>(&self, other: &SbRef<U>) -> bool
    where
        U: SuperBlock + ?Sized,
    {
        core::ptr::addr_eq(self.0.as_ptr(), other.0.as_ptr())
    }
}

impl<S> SbUse<S>
where
    S: SuperBlock,
{
    pub fn new(info: SuperBlockInfo, backend: S) -> Self {
        Self(Arc::new(SuperBlockComplex {
            info,
            rwsem: RwLock::new(SuperBlockLock(())),
            backend,
        }))
    }

    pub fn new_cyclic(info: SuperBlockInfo, backend_func: impl FnOnce(SbRef<S>) -> S) -> Self {
        Self(Arc::new_cyclic(|weak| SuperBlockComplex {
            info,
            rwsem: RwLock::new(SuperBlockLock(())),
            backend: backend_func(SbRef(weak.clone())),
        }))
    }
}

impl<S> Clone for SbRef<S>
where
    S: SuperBlock + ?Sized,
{
    fn clone(&self) -> Self {
        SbRef(self.0.clone())
    }
}

impl<S> Clone for SbUse<S>
where
    S: SuperBlock + ?Sized,
{
    fn clone(&self) -> Self {
        SbUse(self.0.clone())
    }
}

impl<T, U> CoerceUnsized<SbRef<U>> for SbRef<T>
where
    T: SuperBlock + Unsize<U> + ?Sized,
    U: SuperBlock + ?Sized,
{
}

impl<T, U> CoerceUnsized<SbUse<U>> for SbUse<T>
where
    T: SuperBlock + Unsize<U> + ?Sized,
    U: SuperBlock + ?Sized,
{
}

impl<S> Deref for SbUse<S>
where
    S: SuperBlock + ?Sized,
{
    type Target = SuperBlockComplex<S>;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}
