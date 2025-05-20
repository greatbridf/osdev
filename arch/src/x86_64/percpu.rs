use super::wrmsr;
use crate::x86_64::mm::PAGE_SIZE;
use core::{
    alloc::Layout,
    arch::asm,
    cell::UnsafeCell,
    ptr::{null_mut, NonNull},
    sync::atomic::{AtomicPtr, Ordering},
};

pub const MAX_CPUS: usize = 256;

#[repr(align(4096))]
struct PercpuData(UnsafeCell<()>); // Not `Sync`.

pub struct PercpuArea {
    data: NonNull<PercpuData>,
}

static PERCPU_POINTERS: [AtomicPtr<PercpuData>; MAX_CPUS] =
    [const { AtomicPtr::new(null_mut()) }; MAX_CPUS];

impl PercpuArea {
    fn page_count() -> usize {
        extern "C" {
            static PERCPU_PAGES: usize;
        }
        // SAFETY: `PERCPU_PAGES` is defined in linker script and never change.
        let page_count = unsafe { PERCPU_PAGES };
        assert_ne!(page_count, 0);
        page_count
    }

    fn data_start() -> NonNull<u8> {
        extern "C" {
            fn _PERCPU_DATA_START();
        }

        NonNull::new(_PERCPU_DATA_START as usize as *mut _)
            .expect("Percpu data should not be null.")
    }

    fn layout() -> Layout {
        Layout::from_size_align(Self::page_count() * PAGE_SIZE, PAGE_SIZE).expect("Invalid layout.")
    }

    pub fn new<F>(allocate: F) -> Self
    where
        F: FnOnce(Layout) -> NonNull<u8>,
    {
        let data_pointer = allocate(Self::layout());

        unsafe {
            // SAFETY: The `data_pointer` is of valid length and properly aligned.
            data_pointer
                .copy_from_nonoverlapping(Self::data_start(), Self::page_count() * PAGE_SIZE);
        }

        Self {
            data: data_pointer.cast(),
        }
    }

    /// Set up the percpu area for the current CPU.
    pub fn setup(&self) {
        wrmsr(0xC0000101, self.data.as_ptr() as u64);

        unsafe {
            // SAFETY: %gs:0 points to the start of the percpu area.
            asm!(
                "movq {}, %gs:0",
                in(reg) self.data.as_ptr(),
                options(nostack, preserves_flags, att_syntax)
            );
        }
    }

    pub fn register(self: Self, cpuid: usize) {
        PERCPU_POINTERS[cpuid].store(self.data.as_ptr(), Ordering::Release);
    }

    pub fn get_for(cpuid: usize) -> Option<NonNull<()>> {
        let pointer = PERCPU_POINTERS[cpuid].load(Ordering::Acquire);
        NonNull::new(pointer.cast())
    }
}
