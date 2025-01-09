pub mod dataflow;

#[allow(unused_imports)]
pub use dataflow::{UserBuffer, UserString};

pub type UserPointer<'a, T> = dataflow::UserPointer<'a, T, true>;
pub type UserPointerMut<'a, T> = dataflow::UserPointer<'a, T, false>;
