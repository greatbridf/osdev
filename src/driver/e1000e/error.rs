use crate::{
    kernel::constants::{EAGAIN, EINVAL, EIO, EOVERFLOW},
    net::NetError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum E1000eError {
    TooManyDescriptors,
    TooBigDataSize,
    NoFreeDescriptor,
    DeviceNotReady,
    UnsupportedSpeed,
}

impl From<E1000eError> for u32 {
    fn from(err: E1000eError) -> Self {
        match err {
            E1000eError::TooManyDescriptors => EINVAL,
            E1000eError::TooBigDataSize => EOVERFLOW,
            E1000eError::NoFreeDescriptor => EAGAIN,
            E1000eError::DeviceNotReady => EIO,
            E1000eError::UnsupportedSpeed => EINVAL,
        }
    }
}

impl From<E1000eError> for NetError {
    fn from(err: E1000eError) -> Self {
        match err {
            E1000eError::TooManyDescriptors => NetError::Unsupported,
            E1000eError::TooBigDataSize => NetError::Unsupported,
            E1000eError::NoFreeDescriptor => NetError::DeviceBusy,
            E1000eError::DeviceNotReady => NetError::IoFailure,
            E1000eError::UnsupportedSpeed => NetError::Unsupported,
        }
    }
}
