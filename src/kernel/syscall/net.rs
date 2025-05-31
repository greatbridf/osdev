use crate::kernel::constants::EINVAL;
use crate::prelude::*;

#[eonix_macros::define_syscall(0x167)]
fn socket(_domain: u32, _socket_type: u32, _protocol: u32) -> KResult<u32> {
    Err(EINVAL)
}
