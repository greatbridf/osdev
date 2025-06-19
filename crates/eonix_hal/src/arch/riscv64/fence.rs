use core::{
    arch::asm,
    sync::atomic::{compiler_fence, Ordering},
};

#[doc(hidden)]
/// Issues a full memory barrier.
///
/// Ensures all memory operations issued before the fence are globally
/// visible before any memory operations issued after the fence.
pub fn memory_barrier() {
    unsafe {
        // A full memory barrier to prevent the compiler from reordering.
        compiler_fence(Ordering::SeqCst);

        // rw for both predecessor and successor: read-write, read-write
        asm!("fence rw, rw", options(nostack, nomem, preserves_flags));

        // A full memory barrier to prevent the compiler from reordering.
        compiler_fence(Ordering::SeqCst);
    }
}

#[doc(hidden)]
/// Issues a read memory barrier.
///
/// Ensures all memory loads issued before the fence are globally
/// visible before any memory loads issued after the fence.
pub fn read_memory_barrier() {
    unsafe {
        // A full memory barrier to prevent the compiler from reordering.
        compiler_fence(Ordering::SeqCst);

        // r for both predecessor and successor: read, read
        asm!("fence r, r", options(nostack, nomem, preserves_flags));

        // A full memory barrier to prevent the compiler from reordering.
        compiler_fence(Ordering::SeqCst);
    }
}

#[doc(hidden)]
/// Issues a write memory barrier.
///
/// Ensures all memory stores issued before the fence are globally
/// visible before any memory stores issued after the fence.
pub fn write_memory_barrier() {
    unsafe {
        // A full memory barrier to prevent the compiler from reordering.
        compiler_fence(Ordering::SeqCst);

        // w for both predecessor and successor: write, write
        asm!("fence w, w", options(nostack, nomem, preserves_flags));

        // A full memory barrier to prevent the compiler from reordering.
        compiler_fence(Ordering::SeqCst);
    }
}
