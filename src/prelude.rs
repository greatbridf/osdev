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

pub(crate) use crate::kernel::console::{
    print, println, println_debug, println_fatal, println_info, println_trace, println_warn,
};

pub(crate) use alloc::{boxed::Box, string::String, vec, vec::Vec};

pub(crate) use core::{fmt::Write, marker::PhantomData, str};

pub use crate::sync::Spin;
