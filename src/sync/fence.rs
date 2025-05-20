use core::sync::atomic::{compiler_fence, Ordering};

/// A strong memory barrier that prevents reordering of memory operations.
pub fn memory_barrier() {
    // A full memory barrier to prevent the compiler from reordering.
    compiler_fence(Ordering::SeqCst);

    arch::memory_barrier();

    // A full memory barrier to prevent the compiler from reordering.
    compiler_fence(Ordering::SeqCst);
}

/// A read memory barrier that prevents reordering of read operations.
pub fn read_memory_barrier() {
    // A full memory barrier to prevent the compiler from reordering.
    compiler_fence(Ordering::SeqCst);

    arch::read_memory_barrier();

    // A read memory barrier to prevent the compiler from reordering.
    compiler_fence(Ordering::SeqCst);
}

/// A write memory barrier that prevents reordering of write operations.
pub fn write_memory_barrier() {
    // A full memory barrier to prevent the compiler from reordering.
    compiler_fence(Ordering::SeqCst);

    arch::write_memory_barrier();

    // A write memory barrier to prevent the compiler from reordering.
    compiler_fence(Ordering::SeqCst);
}
