use crate::kernel::constants::EINVAL;
use crate::prelude::*;
use posix_types::syscall_no::*;

const AF_INET: u32 = 2; // IPv4

const SOCK_STREAM: u32 = 1; // TCP
const SOCK_RAW: u32 = 3; // Raw socket

const IPPROTO_TCP: u32 = 6; // TCP protocol
const IPPROTO_ICMP: u32 = 1; // ICMP protocol

#[eonix_macros::define_syscall(SYS_SOCKET)]
fn socket(_domain: u32, _socket_type: u32, _protocol: u32) -> KResult<u32> {
    println_info!(
        "socket called with domain: {}, type: {}, protocol: {}",
        _domain,
        _socket_type,
        _protocol
    );

    Err(EINVAL)
}

pub fn keep_alive() {}
