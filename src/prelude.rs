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

pub(crate) use dont_check;

pub use crate::bindings::root as bindings;

pub(crate) use crate::kernel::console::{
    print, println, println_debug, println_fatal, println_info, println_trace, println_warn,
};

pub(crate) use alloc::{boxed::Box, string::String, vec, vec::Vec};

pub(crate) use core::{any::Any, fmt::Write, marker::PhantomData, str};

pub use crate::sync::{Mutex, RwLock, Spin};

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
