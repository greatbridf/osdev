use crate::kernel::task::Thread;

mod file_rw;
mod mm;
mod net;
mod procops;
mod sysinfo;

const MAX_SYSCALL_NO: usize = 512;

pub struct SyscallNoReturn;

pub struct SyscallHandler {
    pub handler: fn(&Thread, [usize; 6]) -> Option<usize>,
    pub name: &'static str,
}

pub trait FromSyscallArg {
    fn from_arg(value: usize) -> Self;
}

pub trait SyscallRetVal {
    fn into_retval(self) -> Option<usize>;
}

impl<T> SyscallRetVal for Result<T, u32>
where
    T: SyscallRetVal,
{
    fn into_retval(self) -> Option<usize> {
        match self {
            Ok(v) => v.into_retval(),
            Err(e) => Some((-(e as isize)) as usize),
        }
    }
}

impl SyscallRetVal for () {
    fn into_retval(self) -> Option<usize> {
        Some(0)
    }
}

impl SyscallRetVal for u32 {
    fn into_retval(self) -> Option<usize> {
        Some(self as usize)
    }
}

impl SyscallRetVal for usize {
    fn into_retval(self) -> Option<usize> {
        Some(self)
    }
}

impl SyscallRetVal for SyscallNoReturn {
    fn into_retval(self) -> Option<usize> {
        None
    }
}

impl FromSyscallArg for u64 {
    fn from_arg(value: usize) -> u64 {
        value as u64
    }
}

impl FromSyscallArg for u32 {
    fn from_arg(value: usize) -> u32 {
        value as u32
    }
}

impl FromSyscallArg for i32 {
    fn from_arg(value: usize) -> i32 {
        value as i32
    }
}

impl FromSyscallArg for usize {
    fn from_arg(value: usize) -> usize {
        value
    }
}

impl<T> FromSyscallArg for *const T {
    fn from_arg(value: usize) -> *const T {
        value as *const T
    }
}

impl<T> FromSyscallArg for *mut T {
    fn from_arg(value: usize) -> *mut T {
        value as *mut T
    }
}

pub fn syscall_handlers() -> &'static [SyscallHandler; MAX_SYSCALL_NO] {
    extern "C" {
        #[allow(improper_ctypes)]
        static SYSCALL_HANDLERS: [SyscallHandler; MAX_SYSCALL_NO];
    }

    unsafe {
        // SAFETY: `SYSCALL_HANDLERS` is defined in linker script.
        &SYSCALL_HANDLERS
    }
}
