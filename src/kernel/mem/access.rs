use core::{num::NonZero, ptr::NonNull};
use eonix_hal::mm::ArchPhysAccess;
use eonix_mm::address::{Addr as _, PAddr, PhysAccess as _PhysAccess, VAddr};
use eonix_mm::paging::PFN;

use super::page_alloc::RawPagePtr;

const PHYS_OFFSET: usize = 0xffff_ff00_0000_0000;

/// A block of memory starting at a non-zero address and having a specific length.
///
/// This struct is used to represent a memory block that can be accessed
/// in the kernel space.
pub struct MemoryBlock {
    addr: NonZero<usize>,
    len: usize,
}

pub trait AsMemoryBlock {
    /// Translate the physical page the page object pointing to into kernel
    /// accessible pointer. Use it with care.
    fn as_memblk(&self) -> MemoryBlock;
}

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

impl MemoryBlock {
    /// Create a new `MemoryBlock` with the given address and length.
    ///
    /// # Safety
    /// The caller must ensure that the address is valid.
    /// Otherwise, it may lead to undefined behavior.
    pub unsafe fn new(addr: NonZero<usize>, len: usize) -> Self {
        Self { addr, len }
    }

    /// Get the start address of the memory block.
    #[allow(dead_code)]
    pub fn addr(&self) -> NonZero<usize> {
        self.addr
    }

    /// Get the length of the memory block.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Split the memory block into two parts at the given offset.
    pub fn split_at(&self, at: usize) -> (Self, Self) {
        if at > self.len {
            panic!("Out of bounds");
        }

        let rhs_start = self.addr.checked_add(at).expect("Overflow");

        let lhs = unsafe { Self::new(self.addr, at) };
        let rhs = unsafe { Self::new(rhs_start, self.len - at) };

        (lhs, rhs)
    }

    /// Provide a pointer to the data.
    ///
    /// # Safety
    /// Using the returned pointer is undefined behavior if the address is not
    ///  properly aligned or the size is not equal to the size of `T`.
    pub unsafe fn as_ptr_unchecked<T>(&self) -> NonNull<T> {
        // SAFETY: `self.addr` is a non-zero value.
        NonNull::new_unchecked(self.addr.get() as *mut T)
    }

    /// Provide a pointer to the data.
    ///
    /// # Panic
    /// Panic if the address is not properly aligned.
    pub fn as_ptr<T>(&self) -> NonNull<T> {
        let alignment = align_of::<T>();

        if self.addr.get() % alignment != 0 {
            panic!("Alignment error");
        }

        unsafe {
            // SAFETY: We've checked that `self.addr` is properly aligned.
            self.as_ptr_unchecked()
        }
    }

    /// Provide a pointer to the bytes.
    pub fn as_byte_ptr(&self) -> NonNull<u8> {
        unsafe {
            // SAFETY: No alignment check is needed for bytes.
            self.as_ptr_unchecked()
        }
    }

    /// Provide immutable access to the data it pointed to.
    ///
    /// # Safety
    /// This function is unsafe because it returns an immutable reference with
    /// a created lifetime.
    ///
    /// The caller must ensure that the data has no other mutable aliases while
    /// the reference is in use. Otherwise, it may lead to undefined behavior.
    pub unsafe fn as_bytes<'a>(&self) -> &'a [u8] {
        core::slice::from_raw_parts(self.as_ptr_unchecked().as_ptr(), self.len)
    }

    /// Provide mutable access to the data it pointed to.
    ///
    /// # Panic
    /// Panic if the address is not properly aligned or the size is not
    /// equal to the size of `T`.
    ///
    /// # Safety
    /// This function is unsafe because it returns a mutable reference with a
    /// created lifetime.
    ///
    /// The caller must ensure that the data has no other immutable or mutable
    /// aliases while the reference is in use.
    /// Otherwise, it may lead to undefined behavior.
    pub unsafe fn as_bytes_mut<'a>(&mut self) -> &'a mut [u8] {
        core::slice::from_raw_parts_mut(self.as_ptr_unchecked().as_ptr(), self.len)
    }
}

impl PhysAccess for PAddr {
    unsafe fn as_ptr<T>(&self) -> NonNull<T> {
        ArchPhysAccess::as_ptr(*self)
    }
}

pub trait RawPageAccess {
    /// Translate the address belonged RawPage ptr
    /// Use it with care.
    ///
    /// # Panic
    /// If the address is not properly aligned.
    ///
    /// # Safety
    /// the address must be kernel accessible pointer
    unsafe fn as_raw_page(&self) -> RawPagePtr;
}

impl RawPageAccess for VAddr {
    unsafe fn as_raw_page(&self) -> RawPagePtr {
        let pfn: PFN = PAddr::from(self.addr() - PHYS_OFFSET).into();
        RawPagePtr::from(pfn)
    }
}
