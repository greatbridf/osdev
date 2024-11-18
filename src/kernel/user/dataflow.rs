use core::{arch::asm, ffi::CStr};

use bindings::{EFAULT, EINVAL};

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
    pub fn new(ptr: *mut T) -> KResult<Self> {
        let pointer = CheckedUserPointer::new(ptr as *const u8, core::mem::size_of::<T>())?;

        Ok(Self {
            pointer,
            _phantom: core::marker::PhantomData,
        })
    }

    pub fn new_vaddr(vaddr: usize) -> KResult<Self> {
        Self::new(vaddr as *mut T)
    }

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

    pub fn get_mut<T>(&self) -> *mut T {
        self.ptr as *mut T
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
        might_sleep!();

        if total > self.len {
            return Err(EINVAL);
        }

        let error_bytes: usize;
        unsafe {
            asm!(
                "2:",
                "rep movsb",
                "3:",
                "nop",
                ".pushsection .fix",
                ".align 32",
                ".quad 2b",  // instruction address
                ".quad 3b - 2b",  // instruction length
                ".quad 3b",  // fix jump address
                ".quad 0x3", // type: load
                ".popsection",
                inout("rcx") total => error_bytes,
                inout("rsi") self.ptr => _,
                inout("rdi") buffer => _,
            )
        }

        if error_bytes != 0 {
            Err(EFAULT)
        } else {
            Ok(())
        }
    }

    /// # Might Sleep
    pub fn write(&self, data: *mut (), total: usize) -> KResult<()> {
        might_sleep!();

        if total > self.len {
            return Err(EINVAL);
        }

        // TODO: align to 8 bytes when doing copy for performance
        let error_bytes: usize;
        unsafe {
            asm!(
                "2:",
                "rep movsb",
                "3:",
                "nop",
                ".pushsection .fix",
                ".align 32",
                ".quad 2b",  // instruction address
                ".quad 3b - 2b",  // instruction length
                ".quad 3b",  // fix jump address
                ".quad 0x1", // type: store
                ".popsection",
                inout("rcx") total => error_bytes,
                inout("rsi") data => _,
                inout("rdi") self.ptr => _,
            )
        };

        if error_bytes != 0 {
            return Err(EFAULT);
        }

        Ok(())
    }

    /// # Might Sleep
    pub fn zero(&self) -> KResult<()> {
        might_sleep!();

        if self.len == 0 {
            return Ok(());
        }

        // TODO: align to 8 bytes when doing copy for performance
        let error_bytes: usize;
        unsafe {
            asm!(
                "2:",
                "rep stosb",
                "3:",
                "nop",
                ".pushsection .fix",
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
            )
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
        might_sleep!();

        let to_write = data.len().min(self.remaining());
        if to_write == 0 {
            return Ok(FillResult::Full);
        }

        self.ptr.write(data.as_ptr() as *mut (), to_write)?;
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
        might_sleep!();

        const MAX_LEN: usize = 4096;
        // TODO
        let ptr = CheckedUserPointer::new(ptr, MAX_LEN)?;

        let result: usize;
        unsafe {
            asm!(
                "2:",
                "mov al, byte ptr [rdx]",
                "4:",
                "test al, al",
                "jz 3f",
                "add rdx, 1",
                "loop 2b",
                "3:",
                "nop",
                ".pushsection .fix",
                ".align 32",
                ".quad 2b",  // instruction address
                ".quad 4b - 2b",  // instruction length
                ".quad 3b",  // fix jump address
                ".quad 0x2", // type: string
                ".popsection",
                in("rdx") ptr.get_const::<u8>(),
                inout("rcx") MAX_LEN => result,
            )
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
