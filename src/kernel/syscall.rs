use super::task::ThreadAlloc;
use crate::kernel::task::Thread;
use alloc::boxed::Box;
use core::{future::Future, marker::PhantomData, ops::Deref, pin::Pin};
use eonix_mm::address::{Addr, VAddr};
use eonix_sync::LazyLock;
use posix_types::ctypes::PtrT;

pub mod file_rw;
pub mod mm;
pub mod net;
pub mod procops;
pub mod sysinfo;

const MAX_SYSCALL_NO: usize = 512;

#[derive(Debug, Clone, Copy)]
pub struct SyscallNoReturn;

#[derive(Clone, Copy)]
pub struct User<T>(VAddr, PhantomData<T>);

#[derive(Clone, Copy)]
pub struct UserMut<T>(VAddr, PhantomData<T>);

#[repr(C)]
pub(self) struct RawSyscallHandler {
    no: usize,
    handler: for<'thd, 'alloc> fn(
        &'thd Thread,
        ThreadAlloc<'alloc>,
        [usize; 6],
    ) -> Pin<
        Box<dyn Future<Output = Option<usize>> + Send + 'thd, ThreadAlloc<'alloc>>,
    >,
    name: &'static str,
}

pub struct SyscallHandler {
    pub handler: for<'thd, 'alloc> fn(
        &'thd Thread,
        ThreadAlloc<'alloc>,
        [usize; 6],
    ) -> Pin<
        Box<dyn Future<Output = Option<usize>> + Send + 'thd, ThreadAlloc<'alloc>>,
    >,
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

impl SyscallRetVal for i32 {
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

impl SyscallRetVal for SyscallNoReturn {
    fn into_retval(self) -> Option<usize> {
        None
    }
}

impl<T> SyscallRetVal for User<T> {
    fn into_retval(self) -> Option<usize> {
        Some(self.0.addr())
    }
}

impl<T> SyscallRetVal for UserMut<T> {
    fn into_retval(self) -> Option<usize> {
        Some(self.0.addr())
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

impl FromSyscallArg for isize {
    fn from_arg(value: usize) -> isize {
        value as isize
    }
}

impl FromSyscallArg for PtrT {
    fn from_arg(value: usize) -> Self {
        PtrT::new(value).expect("Invalid user pointer value")
    }
}

impl<T> FromSyscallArg for User<T> {
    fn from_arg(value: usize) -> User<T> {
        User(VAddr::from(value), PhantomData)
    }
}

impl<T> FromSyscallArg for UserMut<T> {
    fn from_arg(value: usize) -> UserMut<T> {
        UserMut(VAddr::from(value), PhantomData)
    }
}

impl<T> User<T> {
    pub const fn new(addr: VAddr) -> Self {
        Self(addr, PhantomData)
    }

    pub const fn with_addr(addr: usize) -> Self {
        Self::new(VAddr::from(addr))
    }

    pub const fn null() -> Self {
        Self(VAddr::NULL, PhantomData)
    }

    pub fn is_null(&self) -> bool {
        self.0.addr() == 0
    }

    pub const fn cast<U>(self) -> User<U> {
        User(self.0, PhantomData)
    }

    pub fn offset(self, off: isize) -> Self {
        Self(
            VAddr::from(
                self.0
                    .addr()
                    .checked_add_signed(off)
                    .expect("offset overflow"),
            ),
            PhantomData,
        )
    }

    pub const unsafe fn as_mut(self) -> UserMut<T> {
        UserMut(self.0, PhantomData)
    }
}

impl<T> UserMut<T> {
    pub const fn new(addr: VAddr) -> Self {
        Self(addr, PhantomData)
    }

    pub const fn with_addr(addr: usize) -> Self {
        Self::new(VAddr::from(addr))
    }

    pub const fn null() -> Self {
        Self(VAddr::NULL, PhantomData)
    }

    pub fn is_null(&self) -> bool {
        self.0.addr() == 0
    }

    pub const fn cast<U>(self) -> UserMut<U> {
        UserMut(self.0, PhantomData)
    }

    pub fn offset(self, off: isize) -> Self {
        Self(
            VAddr::from(
                self.0
                    .addr()
                    .checked_add_signed(off)
                    .expect("offset overflow"),
            ),
            PhantomData,
        )
    }

    pub const fn as_const(self) -> User<T> {
        User(self.0, PhantomData)
    }

    pub const fn vaddr(&self) -> VAddr {
        self.0
    }
}

impl<T> Deref for User<T> {
    type Target = VAddr;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Deref for UserMut<T> {
    type Target = VAddr;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> core::fmt::Debug for User<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            VAddr::NULL => write!(f, "User(NULL)"),
            _ => write!(f, "User({:#018x?})", self.0.addr()),
        }
    }
}

impl<T> core::fmt::Debug for UserMut<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            VAddr::NULL => write!(f, "UserMut(NULL)"),
            _ => write!(f, "UserMut({:#018x?})", self.0.addr()),
        }
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
