#[allow(dead_code)]
pub type KResult<T> = Result<T, u32>;

pub(crate) use alloc::boxed::Box;
pub(crate) use alloc::string::String;
pub(crate) use alloc::vec;
pub(crate) use alloc::vec::Vec;
pub(crate) use core::fmt::Write;
pub(crate) use core::marker::PhantomData;
pub(crate) use core::str;

pub use crate::sync::Spin;
