use crate::{
    kernel::{constants::EIO, mem::PhysAccess as _},
    sync::fence::memory_barrier,
    KResult,
};
use core::ptr::NonNull;
use eonix_mm::address::PAddr;

pub struct Register<T: Copy> {
    addr: NonNull<T>,
}

unsafe impl<T: Copy> Send for Register<T> {}
unsafe impl<T: Copy> Sync for Register<T> {}

impl<T: Copy> Register<T> {
    pub fn new(addr: PAddr) -> Self {
        Self {
            addr: unsafe { addr.as_ptr() },
        }
    }

    pub fn read(&self) -> T {
        unsafe { self.addr.as_ptr().read_volatile() }
    }

    pub fn write(&self, value: T) {
        unsafe { self.addr.as_ptr().write_volatile(value) }
    }

    pub fn read_once(&self) -> T {
        let val = unsafe { self.addr.as_ptr().read_volatile() };
        memory_barrier();
        val
    }

    pub fn write_once(&self, value: T) {
        let val = unsafe { self.addr.as_ptr().write_volatile(value) };
        memory_barrier();
        val
    }
}

impl Register<u32> {
    pub fn spinwait_clear(&self, mask: u32) -> KResult<()> {
        const SPINWAIT_MAX: usize = 1000;

        for _ in 0..SPINWAIT_MAX {
            if self.read() & mask == 0 {
                memory_barrier();
                return Ok(());
            }
        }

        memory_barrier();
        Err(EIO)
    }
}
