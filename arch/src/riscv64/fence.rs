use core::arch::asm;

#[doc(hidden)]
/// Issues a full memory barrier.
///
/// Ensures all memory operations issued before the fence are globally
/// visible before any memory operations issued after the fence.
///
/// **NO COMPILER BARRIERS** are emitted by this function.
pub fn memory_barrier() {
    unsafe {
        // rw for both predecessor and successor: read-write, read-write
        asm!("fence rw, rw", options(nostack, nomem, preserves_flags));
    }
}

#[doc(hidden)]
/// Issues a read memory barrier.
///
/// Ensures all memory loads issued before the fence are globally
/// visible before any memory loads issued after the fence.
///
/// **NO COMPILER BARRIERS** are emitted by this function.
pub fn read_memory_barrier() {
    unsafe {
        // r for both predecessor and successor: read, read
        asm!("fence r, r", options(nostack, nomem, preserves_flags));
    }
}

#[doc(hidden)]
/// Issues a write memory barrier.
///
/// Ensures all memory stores issued before the fence are globally
/// visible before any memory stores issued after the fence.
///
/// **NO COMPILER BARRIERS** are emitted by this function.
pub fn write_memory_barrier() {
    unsafe {
        // w for both predecessor and successor: write, write
        asm!("fence w, w", options(nostack, nomem, preserves_flags));
    }
}

#[doc(hidden)]
/// Issues a TLB invalidation memory barrier.
///
/// Typically used after modifying page tables.
pub fn tlb_flush() {
    unsafe {
        asm!("sfence.vma zero, zero", options(nostack, nomem, preserves_flags));
    }
}
