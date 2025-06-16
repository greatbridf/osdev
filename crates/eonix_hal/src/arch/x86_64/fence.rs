use core::{
    arch::asm,
    sync::atomic::{compiler_fence, Ordering},
};

#[doc(hidden)]
/// Issues a full memory barrier.
pub fn memory_barrier() {
    unsafe {
        // A full memory barrier to prevent the compiler from reordering.
        compiler_fence(Ordering::SeqCst);

        asm!("mfence", options(nostack, nomem, preserves_flags));

        // A full memory barrier to prevent the compiler from reordering.
        compiler_fence(Ordering::SeqCst);
    }
}

#[doc(hidden)]
/// Issues a read memory barrier.
pub fn read_memory_barrier() {
    unsafe {
        // A full memory barrier to prevent the compiler from reordering.
        compiler_fence(Ordering::SeqCst);

        asm!("lfence", options(nostack, nomem, preserves_flags));

        // A full memory barrier to prevent the compiler from reordering.
        compiler_fence(Ordering::SeqCst);
    }
}

#[doc(hidden)]
/// Issues a write memory barrier.
pub fn write_memory_barrier() {
    unsafe {
        // A full memory barrier to prevent the compiler from reordering.
        compiler_fence(Ordering::SeqCst);

        asm!("sfence", options(nostack, nomem, preserves_flags));

        // A full memory barrier to prevent the compiler from reordering.
        compiler_fence(Ordering::SeqCst);
    }
}
