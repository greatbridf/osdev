use crate::kernel::constants::{EFAULT, EINVAL};
use core::{arch::asm, ffi::CStr};
use eonix_preempt::assert_preempt_enabled;

use crate::{
    io::{Buffer, FillResult},
    prelude::*,
};

pub struct CheckedUserPointer {
    ptr: *const u8,
    len: usize,
}

pub struct UserBuffer<'lt> {
    ptr: CheckedUserPointer,
    size: usize,
    cur: usize,
    _phantom: core::marker::PhantomData<&'lt ()>,
}

pub struct UserString<'lt> {
    ptr: CheckedUserPointer,
    len: usize,
    _phantom: core::marker::PhantomData<&'lt ()>,
}

pub struct UserPointer<'a, T: Copy, const CONST: bool> {
    pointer: CheckedUserPointer,
    _phantom: core::marker::PhantomData<&'a T>,
}

impl<'a, T: Copy, const CONST: bool> UserPointer<'a, T, CONST> {
    pub fn new(ptr: *const T) -> KResult<Self> {
        let pointer = CheckedUserPointer::new(ptr as *const u8, core::mem::size_of::<T>())?;

        Ok(Self {
            pointer,
            _phantom: core::marker::PhantomData,
        })
    }

    pub fn new_vaddr(vaddr: usize) -> KResult<Self> {
        Self::new(vaddr as *mut T)
    }

    /// # Might Sleep
    pub fn read(&self) -> KResult<T> {
        let mut value = core::mem::MaybeUninit::<T>::uninit();
        self.pointer
            .read(value.as_mut_ptr() as *mut (), core::mem::size_of::<T>())?;
        Ok(unsafe { value.assume_init() })
    }

    pub fn offset(&self, offset: isize) -> KResult<Self> {
        let new_vaddr = self.pointer.ptr as isize + offset * size_of::<T>() as isize;
        Self::new_vaddr(new_vaddr as usize)
    }
}

impl<'a, T: Copy> UserPointer<'a, T, false> {
    pub fn write(&self, value: T) -> KResult<()> {
        self.pointer
            .write(&value as *const T as *mut (), core::mem::size_of::<T>())
    }
}

impl CheckedUserPointer {
    pub fn new(ptr: *const u8, len: usize) -> KResult<Self> {
        const USER_MAX_ADDR: usize = 0x7ff_fff_fff_fff;
        let end = (ptr as usize).checked_add(len);
        if ptr.is_null() || end.ok_or(EFAULT)? > USER_MAX_ADDR {
            Err(EFAULT)
        } else {
            Ok(Self { ptr, len })
        }
    }

    pub fn forward(&mut self, offset: usize) {
        assert!(offset <= self.len);
        self.ptr = self.ptr.wrapping_offset(offset as isize);
        self.len -= offset;
    }

    pub fn get_const<T>(&self) -> *const T {
        self.ptr as *const T
    }

    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: the pointer's validity is checked in `new`
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// # Might Sleep
    pub fn read(&self, buffer: *mut (), total: usize) -> KResult<()> {
        assert_preempt_enabled!("UserPointer::read");

        if total > self.len {
            return Err(EINVAL);
        }

        let error_bytes: usize;
        unsafe {
            #[cfg(target_arch = "x86_64")]
            asm!(
                "2:",
                "rep movsb",
                "3:",
                "nop",
                ".pushsection .fix, \"a\", @progbits",
                ".align 32",
                ".quad 2b",      // instruction address
                ".quad 3b - 2b", // instruction length
                ".quad 3b",      // fix jump address
                ".quad 0x3",     // type: load
                ".popsection",
                inout("rcx") total => error_bytes,
                inout("rsi") self.ptr => _,
                inout("rdi") buffer => _,
            );

            #[cfg(target_arch = "riscv64")]
            asm!(
                "2:",
                "lb t0, 0(a1)",
                "sb t0, 0(a2)",
                "addi a1, a1, 1",
                "addi a2, a2, 1",
                "addi a0, a0, -1",
                "bnez a0, 2b",
                "3:",
                "nop",
                ".pushsection .fix, \"a\", @progbits",
                ".align 16",
                ".quad 2b",      // instruction address
                ".quad 3b - 2b", // instruction length
                ".quad 3b",      // fix jump address
                ".quad 0x3",     // type: load
                ".popsection",
                inout("a0") total => error_bytes,
                inout("a1") self.ptr => _,
                inout("a2") buffer => _,
                out("t0") _,
            );
        }

        if error_bytes != 0 {
            Err(EFAULT)
        } else {
            Ok(())
        }
    }

    /// # Might Sleep
    pub fn write(&self, data: *mut (), total: usize) -> KResult<()> {
        assert_preempt_enabled!("UserPointer::write");

        if total > self.len {
            return Err(EINVAL);
        }

        // TODO: align to 8 bytes when doing copy for performance
        let error_bytes: usize;
        unsafe {
            #[cfg(target_arch = "x86_64")]
            asm!(
                "2:",
                "rep movsb",
                "3:",
                "nop",
                ".pushsection .fix, \"a\", @progbits",
                ".align 32",
                ".quad 2b",  // instruction address
                ".quad 3b - 2b",  // instruction length
                ".quad 3b",  // fix jump address
                ".quad 0x1", // type: store
                ".popsection",
                inout("rcx") total => error_bytes,
                inout("rsi") data => _,
                inout("rdi") self.ptr => _,
            );

            #[cfg(target_arch = "riscv64")]
            asm!(
                "2:",
                "lb t0, 0(a1)",
                "sb t0, 0(a2)",
                "addi a1, a1, 1",
                "addi a2, a2, 1",
                "addi a0, a0, -1",
                "bnez a0, 2b",
                "3:",
                "nop",
                ".pushsection .fix, \"a\", @progbits",
                ".align 16",
                ".quad 2b",  // instruction address
                ".quad 3b - 2b",  // instruction length
                ".quad 3b",  // fix jump address
                ".quad 0x1", // type: store
                ".popsection",
                inout("a0") total => error_bytes,
                inout("a1") data => _,
                inout("a2") self.ptr => _,
                out("t0") _,
            );
        };

        if error_bytes != 0 {
            return Err(EFAULT);
        }

        Ok(())
    }

    /// # Might Sleep
    pub fn zero(&self) -> KResult<()> {
        assert_preempt_enabled!("CheckedUserPointer::zero");

        if self.len == 0 {
            return Ok(());
        }

        // TODO: align to 8 bytes when doing copy for performance
        let error_bytes: usize;
        unsafe {
            #[cfg(target_arch = "x86_64")]
            asm!(
                "2:",
                "rep stosb",
                "3:",
                "nop",
                ".pushsection .fix, \"a\", @progbits",
                ".align 32",
                ".quad 2b",  // instruction address
                ".quad 3b - 2b",  // instruction length
                ".quad 3b",  // fix jump address
                ".quad 0x1", // type: store
                ".popsection",
                in("rax") 0,
                inout("rcx") self.len => error_bytes,
                inout("rdi") self.ptr => _,
                options(att_syntax)
            );

            #[cfg(target_arch = "riscv64")]
            asm!(
                "2:",
                "sb zero, 0(a1)",
                "addi a1, a1, 1",
                "addi a0, a0, -1",
                "bnez a0, 2b",
                "3:",
                "nop",
                ".pushsection .fix, \"a\", @progbits",
                ".align 16",
                ".quad 2b",  // instruction address
                ".quad 3b - 2b",  // instruction length
                ".quad 3b",  // fix jump address
                ".quad 0x1", // type: store
                ".popsection",
                inout("a0") self.len => error_bytes,
                inout("a1") self.ptr => _,
            );
        };

        if error_bytes != 0 {
            Err(EFAULT)
        } else {
            Ok(())
        }
    }
}

impl UserBuffer<'_> {
    pub fn new(ptr: *mut u8, size: usize) -> KResult<Self> {
        let ptr = CheckedUserPointer::new(ptr, size)?;

        Ok(Self {
            ptr,
            size,
            cur: 0,
            _phantom: core::marker::PhantomData,
        })
    }

    fn remaining(&self) -> usize {
        self.size - self.cur
    }
}

impl<'lt> Buffer for UserBuffer<'lt> {
    fn total(&self) -> usize {
        self.size
    }

    fn wrote(&self) -> usize {
        self.cur
    }

    /// # Might Sleep
    fn fill(&mut self, data: &[u8]) -> KResult<FillResult> {
        assert_preempt_enabled!("UserBuffer::fill");

        let to_write = data.len().min(self.remaining());
        if to_write == 0 {
            return Ok(FillResult::Full);
        }

        self.ptr.write(data.as_ptr() as *mut (), to_write)?;
        self.ptr.forward(to_write);
        self.cur += to_write;

        if to_write == data.len() {
            Ok(FillResult::Done(to_write))
        } else {
            Ok(FillResult::Partial(to_write))
        }
    }
}

impl<'lt> UserString<'lt> {
    /// # Might Sleep
    pub fn new(ptr: *const u8) -> KResult<Self> {
        assert_preempt_enabled!("UserString::new");

        const MAX_LEN: usize = 4096;
        let ptr = CheckedUserPointer::new(ptr, MAX_LEN)?;

        let result: usize;
        unsafe {
            #[cfg(target_arch = "x86_64")]
            asm!(
                "2:",
                "movb ({ptr}), %al",
                "4:",
                "test %al, %al",
                "jz 3f",
                "add $1, {ptr}",
                "loop 2b",
                "3:",
                "nop",
                ".pushsection .fix, \"a\", @progbits",
                ".align 32",
                ".quad 2b",  // instruction address
                ".quad 4b - 2b",  // instruction length
                ".quad 3b",  // fix jump address
                ".quad 0x2", // type: string
                ".popsection",
                out("al") _,
                inout("rcx") MAX_LEN => result,
                ptr = inout(reg) ptr.ptr => _,
                options(att_syntax),
            );

            #[cfg(target_arch = "riscv64")]
            asm!(
                "2:",
                "lb t0, 0(a1)",
                "4:",
                "beqz t0, 3f",
                "addi a1, a1, 1",
                "addi a0, a0, -1",
                "bnez a0, 2b",
                "3:",
                "nop",
                ".pushsection .fix, \"a\", @progbits",
                ".align 16",
                ".quad 2b",  // instruction address
                ".quad 4b - 2b",  // instruction length
                ".quad 3b",  // fix jump address
                ".quad 0x2", // type: string
                ".popsection",
                out("t0") _,
                inout("a0") MAX_LEN => result,
                inout("a1") ptr.ptr => _,
            );
        };

        if result == 0 {
            Err(EFAULT)
        } else {
            Ok(Self {
                ptr,
                len: MAX_LEN - result,
                _phantom: core::marker::PhantomData,
            })
        }
    }

    pub fn as_cstr(&self) -> &'lt CStr {
        unsafe {
            CStr::from_bytes_with_nul_unchecked(core::slice::from_raw_parts(
                self.ptr.get_const(),
                self.len + 1,
            ))
        }
    }
}
