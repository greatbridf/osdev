use bindings::EINVAL;

use crate::prelude::*;

use super::{define_syscall32, register_syscall};

fn do_socket(_domain: u32, _socket_type: u32, _protocol: u32) -> KResult<u32> {
    Err(EINVAL)
}

define_syscall32!(sys_socket, do_socket, domain: u32, socket_type: u32, protocol: u32);

pub(super) fn register() {
    register_syscall!(0x167, socket);
}
