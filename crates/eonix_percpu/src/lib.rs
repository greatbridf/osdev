#![no_std]

use core::alloc::Layout;
use core::ptr::null_mut;
use core::ptr::NonNull;
use core::sync::atomic::AtomicPtr;
use core::sync::atomic::Ordering;

#[cfg(target_arch = "x86_64")]
pub use eonix_percpu_macros::define_percpu_x86_64 as define_percpu;

#[cfg(target_arch = "x86_64")]
pub use eonix_percpu_macros::define_percpu_shared_x86_64 as define_percpu_shared;

#[cfg(target_arch = "riscv64")]
pub use eonix_percpu_macros::define_percpu_riscv64 as define_percpu;

#[cfg(target_arch = "riscv64")]
pub use eonix_percpu_macros::define_percpu_shared_riscv64 as define_percpu_shared;

const MAX_CPUS: usize = 256;

#[repr(align(16))]
pub struct PercpuData();

pub struct PercpuArea {
    data: NonNull<PercpuData>,
}

static PERCPU_POINTERS: [AtomicPtr<PercpuData>; MAX_CPUS] =
    [const { AtomicPtr::new(null_mut()) }; MAX_CPUS];

impl PercpuArea {
    fn len() -> usize {
        unsafe extern "C" {
            fn PERCPU_LENGTH();
        }
        let len = PERCPU_LENGTH as usize;

        assert_ne!(len, 0, "Percpu length should not be zero.");
        len
    }

    fn data_start() -> NonNull<u8> {
        unsafe extern "C" {
            fn PERCPU_DATA_START();
        }

        let addr = PERCPU_DATA_START as usize;
        NonNull::new(addr as *mut _).expect("Percpu data should not be null.")
    }

    fn layout() -> Layout {
        Layout::from_size_align(Self::len(), align_of::<PercpuData>()).expect("Invalid layout.")
    }

    pub fn new<F>(allocate: F) -> Self
    where
        F: FnOnce(Layout) -> NonNull<u8>,
    {
        let data_pointer = allocate(Self::layout());

        unsafe {
            // SAFETY: The `data_pointer` is of valid length and properly aligned.
            data_pointer.copy_from_nonoverlapping(Self::data_start(), Self::len());
        }

        Self {
            data: data_pointer.cast(),
        }
    }

    pub fn register(self, cpuid: usize) {
        PERCPU_POINTERS[cpuid].store(self.data.as_ptr(), Ordering::Release);
    }

    pub fn get_for(cpuid: usize) -> Option<NonNull<()>> {
        let pointer = PERCPU_POINTERS[cpuid].load(Ordering::Acquire);
        NonNull::new(pointer.cast())
    }

    pub fn setup(&mut self, func: impl FnOnce(NonNull<PercpuData>)) {
        func(self.data)
    }
}
