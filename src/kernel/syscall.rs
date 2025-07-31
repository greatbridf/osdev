use crate::kernel::task::Thread;
use eonix_sync::LazyLock;

pub mod file_rw;
pub mod mm;
pub mod net;
pub mod procops;
pub mod sysinfo;

const MAX_SYSCALL_NO: usize = 512;

#[derive(Debug, Clone, Copy)]
pub struct SyscallNoReturn;

#[repr(C)]
pub(self) struct RawSyscallHandler {
    no: usize,
    handler: fn(&Thread, [usize; 6]) -> Option<usize>,
    name: &'static str,
}

pub struct SyscallHandler {
    pub handler: fn(&Thread, [usize; 6]) -> Option<usize>,
    pub name: &'static str,
}

pub trait FromSyscallArg: core::fmt::Debug {
    fn from_arg(value: usize) -> Self;
}

pub trait SyscallRetVal: core::fmt::Debug {
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

impl SyscallRetVal for isize {
    fn into_retval(self) -> Option<usize> {
        Some(self as usize)
    }
}

impl SyscallRetVal for i32 {
    fn into_retval(self) -> Option<usize> {
        Some(self as usize)
    }
}

impl SyscallRetVal for SyscallNoReturn {
    fn into_retval(self) -> Option<usize> {
        None
    }
}

#[cfg(not(target_arch = "x86_64"))]
impl SyscallRetVal for u64 {
    fn into_retval(self) -> Option<usize> {
        Some(self as usize)
    }
}

#[cfg(not(target_arch = "x86_64"))]
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

static SYSCALL_HANDLERS: LazyLock<[Option<SyscallHandler>; MAX_SYSCALL_NO]> = LazyLock::new(|| {
    extern "C" {
        // SAFETY: `SYSCALL_HANDLERS` is defined in linker script.
        fn RAW_SYSCALL_HANDLERS();
        fn RAW_SYSCALL_HANDLERS_SIZE();
    }

    // DO NOT TOUCH THESE FUNCTIONS!!!
    // THEY ARE USED FOR KEEPING THE OBJECTS NOT STRIPPED BY THE LINKER!!!
    file_rw::keep_alive();
    mm::keep_alive();
    net::keep_alive();
    procops::keep_alive();
    sysinfo::keep_alive();

    let raw_handlers_addr = RAW_SYSCALL_HANDLERS as *const ();
    let raw_handlers_size_byte = RAW_SYSCALL_HANDLERS_SIZE as usize;
    assert!(raw_handlers_size_byte % size_of::<RawSyscallHandler>() == 0);

    let raw_handlers_count = raw_handlers_size_byte / size_of::<RawSyscallHandler>();

    let raw_handlers = unsafe {
        core::slice::from_raw_parts(
            raw_handlers_addr as *const RawSyscallHandler,
            raw_handlers_count,
        )
    };

    let mut handlers = [const { None }; MAX_SYSCALL_NO];

    for &RawSyscallHandler { no, handler, name } in raw_handlers.iter() {
        handlers[no] = Some(SyscallHandler { handler, name })
    }

    handlers
});

pub fn syscall_handlers() -> &'static [Option<SyscallHandler>] {
    SYSCALL_HANDLERS.as_ref()
}
