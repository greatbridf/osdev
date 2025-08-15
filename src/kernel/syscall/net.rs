use core::net::IpAddr;
use core::net::Ipv4Addr;
use core::net::SocketAddr;

use crate::io::Buffer;
use crate::io::IntoStream;
use crate::kernel::constants::{EAFNOSUPPORT, EBADF, EINVAL, ENOTSOCK};
use crate::kernel::syscall::file_rw::IoVec;
use crate::kernel::user::dataflow::CheckedUserPointer;
use crate::kernel::user::UserBuffer;
use crate::kernel::user::UserPointer;
use crate::kernel::user::UserPointerMut;
use crate::kernel::vfs::filearray::FD;
use crate::net::socket::tcp::TcpSocket;
use crate::net::socket::udp::UdpSocket;
use crate::net::socket::SendMetadata;
use crate::prelude::*;
use bytes::Buf;
use eonix_runtime::task::Task;
use posix_types::ctypes::Long;
use posix_types::net::CSocketAddrInet;
use posix_types::net::{
    CSockAddr, MsgHdr, Protocol, SockDomain, SockFlags, SockType, ADDR_MAX_LEN, SOCK_TYPE_MASK,
};
use posix_types::syscall_no::*;

fn read_socket_addr(addr_ptr: *const CSockAddr, addrlen: usize) -> KResult<SocketAddr> {
    if addrlen > ADDR_MAX_LEN || addrlen < 2 {
        // println_debug!("here");
        return Err(EINVAL);
    }

    let raw_sockaddr = UserPointer::new(addr_ptr)?.read()?;

    match SockDomain::try_from(raw_sockaddr.sa_family as u32) {
        Ok(SockDomain::AF_INET) => {
            if addrlen < size_of::<SockDomain>() {
                return Err(EINVAL);
            }

            let mut bytes = raw_sockaddr.bytes.as_slice();
            let port = bytes.get_u16();
            let addr_bits = bytes.get_u32();
            let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::from_bits(addr_bits)), port);
            Ok(socket_addr)
        }
        _ => Err(EAFNOSUPPORT),
    }
}

fn write_socket_addr(
    addr_ptr: *mut CSockAddr,
    addrlen_ptr: *mut u32,
    socket_addr: SocketAddr,
) -> KResult<()> {
    match socket_addr {
        SocketAddr::V4(addr) => {
            let raw_socket = CSocketAddrInet::new(*addr.ip(), addr.port());
            UserPointerMut::new(addr_ptr as *mut CSocketAddrInet)?.write(raw_socket)?;
            UserPointerMut::new(addrlen_ptr)?.write(size_of::<CSocketAddrInet>() as u32)?;
            Ok(())
        }
        SocketAddr::V6(_) => panic!("IPv6 is not supported"),
    }
}

#[eonix_macros::define_syscall(SYS_SOCKET)]
fn socket(domain: u32, type_: u32, protocol: u32) -> KResult<FD> {
    let domain = SockDomain::try_from(domain).map_err(|_| EINVAL)?;
    let sock_type = SockType::try_from(type_ & SOCK_TYPE_MASK).map_err(|_| EINVAL)?;
    let sock_flags = SockFlags::from_bits_truncate(type_ & !SOCK_TYPE_MASK);
    let protocol = Protocol::try_from(protocol).map_err(|_| EINVAL)?;

    let is_nonblock = sock_flags.contains(SockFlags::SOCK_NONBLOCK);

    let socket = match (domain, sock_type) {
        (SockDomain::AF_INET, SockType::SOCK_STREAM) => match protocol {
            Protocol::IPPROTO_IP | Protocol::IPPROTO_TCP => TcpSocket::new(is_nonblock) as _,
            _ => return Err(EAFNOSUPPORT),
        },
        (SockDomain::AF_INET, SockType::SOCK_DGRAM) => match protocol {
            Protocol::IPPROTO_IP | Protocol::IPPROTO_UDP => UdpSocket::new(is_nonblock) as _,
            _ => return Err(EAFNOSUPPORT),
        },
        _ => panic!("unsupported socket"),
    };

    thread.files.socket(socket)
}

#[eonix_macros::define_syscall(SYS_BIND)]
fn bind(sockfd: FD, sockaddr_ptr: *const CSockAddr, addrlen: u32) -> KResult<()> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let socket_addr = read_socket_addr(sockaddr_ptr, addrlen as usize)?;

    socket.bind(socket_addr)
}

#[eonix_macros::define_syscall(SYS_LISTEN)]
fn listen(sockfd: FD, backlog: u32) -> KResult<()> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    socket.listen(backlog as usize)
}

#[eonix_macros::define_syscall(SYS_ACCEPT)]
fn accept(sockfd: FD, sockaddr_ptr: *mut CSockAddr, addrlen_ptr: *mut u32) -> KResult<FD> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let accepted_socket = Task::block_on(socket.accept())?;
    write_socket_addr(
        sockaddr_ptr,
        addrlen_ptr,
        accepted_socket.remote_addr().unwrap(),
    )?;
    thread.files.socket(accepted_socket)
}

#[eonix_macros::define_syscall(SYS_CONNECT)]
fn connect(sockfd: FD, sockaddr_ptr: *const CSockAddr, addrlen: u32) -> KResult<()> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let remote_addr = read_socket_addr(sockaddr_ptr, addrlen as usize)?;

    Task::block_on(socket.connect(remote_addr))
}

#[eonix_macros::define_syscall(SYS_RECVMSG)]
fn recvmsg(sockfd: FD, msghdr_ptr: *mut MsgHdr, flags: u32) -> KResult<usize> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let msghdr = UserPointer::new(msghdr_ptr)?.read()?;

    let mut iov_user = UserPointer::new(msghdr.msg_iov as *mut IoVec)?;
    let iov_buffers = (0..msghdr.msg_iovlen)
        .map(|_| {
            let iov_result = iov_user.read()?;
            iov_user = iov_user.offset(1)?;
            Ok(iov_result)
        })
        .filter_map(|iov_result| match iov_result {
            Err(err) => Some(Err(err)),
            Ok(IoVec {
                len: Long::ZERO, ..
            }) => None,
            Ok(IoVec { base, len }) => Some(UserBuffer::new(base.addr() as *mut u8, len.get())),
        })
        .collect::<KResult<Vec<_>>>()?;

    let mut recv_metadata = None;
    let mut tot = 0usize;
    for mut buffer in iov_buffers.into_iter() {
        let (nread, recv_meta) = Task::block_on(socket.recv(&mut buffer))?;

        if recv_metadata.is_none() {
            recv_metadata = Some(recv_meta);
        } else {
            assert_eq!(recv_metadata, Some(recv_meta));
        }

        tot += nread;
        if nread != buffer.total() {
            break;
        }
    }

    if msghdr.msg_name != 0 {
        let addrlen_ptr = unsafe { msghdr_ptr.byte_add(core::mem::size_of::<usize>()) as *mut u32 };
        write_socket_addr(
            msghdr.msg_name as _,
            addrlen_ptr,
            recv_metadata.unwrap().remote_addr,
        )?;
    }

    Ok(tot)
}

#[eonix_macros::define_syscall(SYS_RECVFROM)]
fn recvfrom(
    sockfd: FD,
    buf: *mut u8,
    len: usize,
    flags: u32,
    srcaddr_ptr: *mut CSockAddr,
    addrlen_ptr: *mut u32,
) -> KResult<usize> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let (ret, recv_meta) = Task::block_on(socket.recv(&mut UserBuffer::new(buf, len)?))?;

    if srcaddr_ptr as usize != 0 {
        write_socket_addr(srcaddr_ptr, addrlen_ptr, recv_meta.remote_addr)?;
    }

    Ok(ret)
}

#[eonix_macros::define_syscall(SYS_SENDMSG)]
fn sendmsg(sockfd: FD, msghdr: *const MsgHdr, flags: u32) -> KResult<usize> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let msghdr = UserPointer::new(msghdr)?.read()?;

    let mut iov_user = UserPointer::new(msghdr.msg_iov as *const IoVec)?;
    let iov_streams = (0..msghdr.msg_iovlen)
        .map(|_| {
            let iov_result = iov_user.read()?;
            iov_user = iov_user.offset(1)?;
            Ok(iov_result)
        })
        .filter_map(|iov_result| match iov_result {
            Err(err) => Some(Err(err)),
            Ok(IoVec {
                len: Long::ZERO, ..
            }) => None,
            Ok(IoVec { base, len }) => Some(
                CheckedUserPointer::new(base.addr() as *mut u8, len.get())
                    .map(|ptr| ptr.into_stream()),
            ),
        })
        .collect::<KResult<Vec<_>>>()?;

    let remote_addr = if msghdr.msg_namelen == 0 {
        None
    } else {
        Some(read_socket_addr(
            msghdr.msg_name as _,
            msghdr.msg_namelen as usize,
        )?)
    };

    let mut tot = 0usize;
    for mut stream in iov_streams.into_iter() {
        let nread = Task::block_on(socket.send(&mut stream, SendMetadata { remote_addr }))?;
        tot += nread;

        if nread == 0 || !stream.is_drained() {
            break;
        }
    }

    Ok(tot)
}

#[eonix_macros::define_syscall(SYS_SENDTO)]
fn sendto(
    sockfd: FD,
    buf: *const u8,
    len: usize,
    flags: u32,
    dstaddr_ptr: *const CSockAddr,
    addrlen: u32,
) -> KResult<usize> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let remote_addr = if addrlen == 0 {
        None
    } else {
        Some(read_socket_addr(dstaddr_ptr, addrlen as usize)?)
    };

    let mut user_stream = CheckedUserPointer::new(buf, len).map(|ptr| ptr.into_stream())?;
    Task::block_on(socket.send(&mut user_stream, SendMetadata { remote_addr }))
}

pub fn keep_alive() {}
