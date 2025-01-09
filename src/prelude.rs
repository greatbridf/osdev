#[allow(dead_code)]
pub type KResult<T> = Result<T, u32>;

macro_rules! dont_check {
    ($arg:expr) => {
        match $arg {
            Ok(_) => (),
            Err(_) => (),
        }
    };
}

use alloc::sync::Arc;
#[allow(unused_imports)]
pub(crate) use dont_check;

#[allow(unused_imports)]
pub use crate::bindings::root as bindings;

#[allow(unused_imports)]
pub(crate) use crate::kernel::console::{
    print, println, println_debug, println_fatal, println_info, println_warn,
};

#[allow(unused_imports)]
pub(crate) use crate::sync::might_sleep;

#[allow(unused_imports)]
pub(crate) use alloc::{boxed::Box, string::String, vec, vec::Vec};

#[allow(unused_imports)]
pub(crate) use core::{any::Any, fmt::Write, marker::PhantomData, str};
use core::{mem::ManuallyDrop, ops::Deref};

#[allow(unused_imports)]
pub use crate::sync::{Locked, Mutex, RwSemaphore, Semaphore, Spin};

pub struct BorrowedArc<'lt, T: ?Sized> {
    arc: ManuallyDrop<Arc<T>>,
    _phantom: PhantomData<&'lt ()>,
}

impl<'lt, T: ?Sized> BorrowedArc<'lt, T> {
    pub fn from_raw(ptr: *const T) -> Self {
        assert!(!ptr.is_null());
        Self {
            arc: ManuallyDrop::new(unsafe { Arc::from_raw(ptr) }),
            _phantom: PhantomData,
        }
    }

    #[allow(dead_code)]
    pub fn new(ptr: &'lt *const T) -> Self {
        assert!(!ptr.is_null());
        Self {
            arc: ManuallyDrop::new(unsafe { Arc::from_raw(*ptr) }),
            _phantom: PhantomData,
        }
    }

    pub fn borrow(&self) -> &'lt T {
        let reference: &T = &self.arc;
        let ptr = reference as *const T;

        // SAFETY: `ptr` is a valid pointer to `T` because `reference` is a valid reference to `T`.
        // `ptr` is also guaranteed to be valid for the lifetime `'lt` because it is derived from
        // `self.arc` which is guaranteed to be valid for the lifetime `'lt`.
        unsafe { ptr.as_ref().unwrap() }
    }
}

impl<'lt, T: ?Sized> Deref for BorrowedArc<'lt, T> {
    type Target = Arc<T>;

    fn deref(&self) -> &Self::Target {
        &self.arc
    }
}

impl<'lt, T: ?Sized> AsRef<Arc<T>> for BorrowedArc<'lt, T> {
    fn as_ref(&self) -> &Arc<T> {
        &self.arc
    }
}

#[allow(dead_code)]
pub trait AsAny: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

macro_rules! impl_any {
    ($t:ty) => {
        impl AsAny for $t {
            fn as_any(&self) -> &dyn Any {
                self
            }

            fn as_any_mut(&mut self) -> &mut dyn Any {
                self
            }
        }
    };
}

macro_rules! addr_of_mut_field {
    ($pointer:expr, $field:ident) => {
        core::ptr::addr_of_mut!((*$pointer).$field)
    };
}

pub(crate) use {addr_of_mut_field, impl_any};
