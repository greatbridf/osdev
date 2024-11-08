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

    pub fn read(&self, buffer: *mut (), total: usize) -> KResult<()> {
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

    fn fill(&mut self, data: &[u8]) -> KResult<FillResult> {
        let remaining = self.remaining();
        if remaining == 0 {
            return Ok(FillResult::Full);
        }

        let data = if data.len() > remaining {
            &data[..remaining]
        } else {
            data
        };

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
                inout("rcx") data.len() => error_bytes,
                inout("rsi") data.as_ptr() => _,
                inout("rdi") self.ptr.get_mut::<u8>().offset(self.cur as isize) => _,
            )
        };

        if error_bytes != 0 {
            return Err(EFAULT);
        }

        self.cur += data.len();
        Ok(FillResult::Done(data.len()))
    }
}

impl<'lt> UserString<'lt> {
    pub fn new(ptr: *const u8) -> KResult<Self> {
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
