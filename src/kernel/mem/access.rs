use core::ptr::NonNull;
use eonix_hal::mm::ArchPhysAccess;
use eonix_mm::address::{PAddr, PhysAccess as _PhysAccess};

pub trait PhysAccess {
    /// Translate the data that this address is pointing to into kernel
    /// accessible pointer. Use it with care.
    ///
    /// # Panic
    /// If the address is not properly aligned.
    ///
    /// # Safety
    /// The caller must ensure that the data is of type `T`.
    /// Otherwise, it may lead to undefined behavior.
    unsafe fn as_ptr<T>(&self) -> NonNull<T>;
}

impl PhysAccess for PAddr {
    unsafe fn as_ptr<T>(&self) -> NonNull<T> {
        ArchPhysAccess::as_ptr(*self)
    }
}
