use crate::{
    io::{IntoStream, Stream},
    kernel::constants::{EFAULT, EINVAL},
};
use core::{arch::asm, ffi::CStr, marker::PhantomData};
use eonix_preempt::assert_preempt_enabled;

use crate::{
    io::{Buffer, FillResult},
    prelude::*,
};

pub struct CheckedUserPointer<'a> {
    ptr: *const u8,
    len: usize,
    _phantom: PhantomData<&'a ()>,
}

unsafe impl<'a> Send for CheckedUserPointer<'a> {}

pub struct UserBuffer<'a> {
    ptr: CheckedUserPointer<'a>,
    size: usize,
    cur: usize,
}

pub struct UserString<'a> {
    ptr: CheckedUserPointer<'a>,
    len: usize,
}

pub struct UserPointer<'a, T: Copy, const CONST: bool> {
    pointer: CheckedUserPointer<'a>,
    _phantom: PhantomData<T>,
}

pub struct UserStream<'a> {
    pointer: CheckedUserPointer<'a>,
    cur: usize,
}

impl<T: Copy, const CONST: bool> UserPointer<'_, T, CONST> {
    pub fn new(ptr: *const T) -> KResult<Self> {
        let pointer = CheckedUserPointer::new(ptr as *const u8, core::mem::size_of::<T>())?;

        Ok(Self {
            pointer,
            _phantom: PhantomData,
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

impl CheckedUserPointer<'_> {
    pub fn new(ptr: *const u8, len: usize) -> KResult<Self> {
        const USER_MAX_ADDR: usize = 0x7ff_fff_fff_fff;
        let end = (ptr as usize).checked_add(len);
        if ptr.is_null() || end.ok_or(EFAULT)? > USER_MAX_ADDR {
            Err(EFAULT)
        } else {
            Ok(Self {
                ptr,
                len,
                _phantom: PhantomData,
            })
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
                ".8byte 2b",      // instruction address
                ".8byte 3b - 2b", // instruction length
                ".8byte 3b",      // fix jump address
                ".8byte 0x3",     // type: load
                ".popsection",
                inout("a0") total => error_bytes,
                inout("a1") self.ptr => _,
                inout("a2") buffer => _,
                out("t0") _,
            );

            #[cfg(target_arch = "loongarch64")]
            asm!(
                "2:",
                "ld.bu  $t0, $a1,  0",
                "st.b   $t0, $a2,  0",
                "addi.d $a1, $a1,  1",
                "addi.d $a2, $a2,  1",
                "addi.d $a0, $a0, -1",
                "bnez   $a0, 2b",
                "3:",
                "nop",
                ".pushsection .fix, \"a\", @progbits",
                ".8byte 2b",      // instruction address
                ".8byte 3b - 2b", // instruction length
                ".8byte 3b",      // fix jump address
                ".8byte 0x3",     // type: load
                ".popsection",
                inout("$a0") total => error_bytes,
                inout("$a1") self.ptr => _,
                inout("$a2") buffer => _,
                out("$t0") _,
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
                ".8byte 2b",  // instruction address
                ".8byte 3b - 2b",  // instruction length
                ".8byte 3b",  // fix jump address
                ".8byte 0x1", // type: store
                ".popsection",
                inout("a0") total => error_bytes,
                inout("a1") data => _,
                inout("a2") self.ptr => _,
                out("t0") _,
            );

            #[cfg(target_arch = "loongarch64")]
            asm!(
                "2:",
                "ld.bu  $t0, $a1,  0",
                "st.b   $t0, $a2,  0",
                "addi.d $a1, $a1,  1",
                "addi.d $a2, $a2,  1",
                "addi.d $a0, $a0, -1",
                "bnez   $a0, 2b",
                "3:",
                "nop",
                ".pushsection .fix, \"a\", @progbits",
                ".8byte 2b",  // instruction address
                ".8byte 3b - 2b",  // instruction length
                ".8byte 3b",  // fix jump address
                ".8byte 0x1", // type: store
                ".popsection",
                inout("$a0") total => error_bytes,
                inout("$a1") data => _,
                inout("$a2") self.ptr => _,
                out("$t0") _,
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
                ".8byte 2b",  // instruction address
                ".8byte 3b - 2b",  // instruction length
                ".8byte 3b",  // fix jump address
                ".8byte 0x1", // type: store
                ".popsection",
                inout("a0") self.len => error_bytes,
                inout("a1") self.ptr => _,
            );

            #[cfg(target_arch = "loongarch64")]
            asm!(
                "2:",
                "sb   $zero, $a1,  0",
                "addi $a1,   $a1,  1",
                "addi $a0,   $a0, -1",
                "bnez $a0,   2b",
                "3:",
                "nop",
                ".pushsection .fix, \"a\", @progbits",
                ".8byte 2b",  // instruction address
                ".8byte 3b - 2b",  // instruction length
                ".8byte 3b",  // fix jump address
                ".8byte 0x1", // type: store
                ".popsection",
                inout("$a0") self.len => error_bytes,
                inout("$a1") self.ptr => _,
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

        Ok(Self { ptr, size, cur: 0 })
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
                ".8byte 2b",  // instruction address
                ".8byte 4b - 2b",  // instruction length
                ".8byte 3b",  // fix jump address
                ".8byte 0x2", // type: string
                ".popsection",
                out("t0") _,
                inout("a0") MAX_LEN => result,
                inout("a1") ptr.ptr => _,
            );

            #[cfg(target_arch = "loongarch64")]
            asm!(
                "2:",
                "ld.bu  $t0, $a1,  0",
                "4:",
                "beqz   $t0, 3f",
                "addi.d $a1, $a1,  1",
                "addi.d $a0, $a0, -1",
                "bnez   $a0, 2b",
                "3:",
                "nop",
                ".pushsection .fix, \"a\", @progbits",
                ".8byte 2b",  // instruction address
                ".8byte 4b - 2b",  // instruction length
                ".8byte 3b",  // fix jump address
                ".8byte 0x2", // type: string
                ".popsection",
                out("$t0") _,
                inout("$a0") MAX_LEN => result,
                inout("$a1") ptr.ptr => _,
            );
        };

        if result == 0 {
            Err(EFAULT)
        } else {
            Ok(Self {
                ptr,
                len: MAX_LEN - result,
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

impl UserStream<'_> {
    pub fn len(&self) -> usize {
        self.pointer.len
    }

    pub fn remaining(&self) -> usize {
        self.pointer.len - self.cur
    }

    pub fn is_drained(&self) -> bool {
        self.cur >= self.pointer.len
    }
}

impl Stream for UserStream<'_> {
    fn total(&self) -> usize {
        self.len()
    }

    fn poll_data<'a>(&mut self, buf: &'a mut [u8]) -> KResult<Option<&'a mut [u8]>> {
        assert_preempt_enabled!("UserStream::poll_data");

        if self.cur >= self.pointer.len {
            return Ok(None);
        }

        let to_read = buf.len().min(self.pointer.len - self.cur);

        self.pointer.read(buf.as_mut_ptr() as *mut (), to_read)?;

        self.pointer.forward(to_read);
        self.cur += to_read;
        Ok(Some(&mut buf[..to_read]))
    }

    fn ignore(&mut self, len: usize) -> KResult<Option<usize>> {
        if self.cur >= self.pointer.len {
            return Ok(None);
        }
        let to_ignore = len.min(self.pointer.len - self.cur);

        self.pointer.forward(to_ignore);
        self.cur += to_ignore;
        Ok(Some(to_ignore))
    }
}

impl<'a> IntoStream for CheckedUserPointer<'a> {
    type Stream = UserStream<'a>;

    fn into_stream(self) -> Self::Stream {
        UserStream {
            pointer: self,
            cur: 0,
        }
    }
}
