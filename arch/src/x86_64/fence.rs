use core::arch::asm;

#[doc(hidden)]
/// Issues a full memory barrier.
///
/// Note that this acts as a low-level operation **ONLY** and should be used with caution.
/// **NO COMPILER BARRIERS** are emitted by this function.
pub fn memory_barrier() {
    unsafe {
        asm!("mfence", options(nostack, nomem, preserves_flags));
    }
}

#[doc(hidden)]
/// Issues a read memory barrier.
///
/// Note that this acts as a low-level operation **ONLY** and should be used with caution.
/// **NO COMPILER BARRIERS** are emitted by this function.
pub fn read_memory_barrier() {
    unsafe {
        asm!("lfence", options(nostack, nomem, preserves_flags));
    }
}

#[doc(hidden)]
/// Issues a write memory barrier.
///
/// Note that this acts as a low-level operation **ONLY** and should be used with caution.
/// **NO COMPILER BARRIERS** are emitted by this function.
pub fn write_memory_barrier() {
    unsafe {
        asm!("sfence", options(nostack, nomem, preserves_flags));
    }
}
