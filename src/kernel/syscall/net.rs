use crate::kernel::constants::EINVAL;
use crate::prelude::*;
use posix_types::syscall_no::*;

#[eonix_macros::define_syscall(SYS_SOCKET)]
fn socket(_domain: u32, _socket_type: u32, _protocol: u32) -> KResult<u32> {
    Err(EINVAL)
}

pub fn keep_alive() {}
