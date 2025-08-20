use core::net::IpAddr;
use core::net::Ipv4Addr;
use core::net::SocketAddr;

use crate::io::Buffer;
use crate::io::IntoStream;
use crate::kernel::constants::{EAFNOSUPPORT, EBADF, EINVAL, ENOTSOCK};
use crate::kernel::syscall::file_rw::IoVec;
use crate::kernel::syscall::User;
use crate::kernel::syscall::UserMut;
use crate::kernel::user::CheckedUserPointer;
use crate::kernel::user::UserBuffer;
use crate::kernel::user::UserPointer;
use crate::kernel::user::UserPointerMut;
use crate::kernel::vfs::filearray::FD;
use crate::net::socket::tcp::TcpSocket;
use crate::net::socket::udp::UdpSocket;
use crate::net::socket::SendMetadata;
use crate::prelude::*;
use bytes::Buf;
use eonix_mm::address::Addr;
use posix_types::ctypes::Long;
use posix_types::net::CSocketAddrInet;
use posix_types::net::{
    CSockAddr, MsgHdr, Protocol, SockDomain, SockFlags, SockType, ADDR_MAX_LEN, SOCK_TYPE_MASK,
};
use posix_types::syscall_no::*;

fn read_socket_addr(addr_ptr: User<CSockAddr>, addrlen: usize) -> KResult<SocketAddr> {
    if addrlen > ADDR_MAX_LEN || addrlen < 2 {
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
    addr_ptr: UserMut<CSockAddr>,
    addrlen_ptr: UserMut<u32>,
    socket_addr: SocketAddr,
) -> KResult<()> {
    match socket_addr {
        SocketAddr::V4(addr) => {
            let raw_socket = CSocketAddrInet::new(*addr.ip(), addr.port());
            UserPointerMut::new(addr_ptr.cast())?.write(raw_socket)?;
            UserPointerMut::new(addrlen_ptr)?.write(size_of::<CSocketAddrInet>() as u32)?;
            Ok(())
        }
        SocketAddr::V6(_) => panic!("IPv6 is not supported"),
    }
}

#[eonix_macros::define_syscall(SYS_SOCKET)]
async fn socket(domain: u32, type_: u32, protocol: u32) -> KResult<FD> {
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
        _ => return Err(EAFNOSUPPORT),
    };

    thread.files.socket(socket)
}

#[eonix_macros::define_syscall(SYS_SETSOCKOPT)]
async fn set_sockopt(
    fd: FD,
    level: u32,
    optname: u32,
    optval: User<u8>,
    optlen: u32,
) -> KResult<()> {
    Ok(())
}

#[eonix_macros::define_syscall(SYS_GETSOCKOPT)]
async fn get_sockopt(
    fd: FD,
    level: u32,
    optname: u32,
    optval: UserMut<u8>,
    optlen: UserMut<u32>,
) -> KResult<()> {
    const SOL_SOCKET: u32 = 1;
    const SQL_TCP: u32 = 6;

    const SNDBUF: u32 = 7;
    const RCVBUF: u32 = 8;

    const TCP_MAXSEG: u32 = 2;

    const MAX_SEGMENT_SIZE: u32 = 1460;

    if level == SOL_SOCKET {
        if optname == SNDBUF || optname == RCVBUF {
            UserPointerMut::new(optval.cast())?.write(65536)?;
            UserPointerMut::new(optlen)?.write(size_of::<u32>() as u32)?;
        }
    } else if level == SQL_TCP {
        if optname == TCP_MAXSEG {
            UserPointerMut::new(optval.cast())?.write(MAX_SEGMENT_SIZE)?;
            UserPointerMut::new(optlen)?.write(size_of::<u32>() as u32)?;
        }
    }

    Ok(())
}

#[eonix_macros::define_syscall(SYS_GETSOCKNAME)]
async fn get_socktname(
    sockfd: FD,
    sockaddr_ptr: UserMut<CSockAddr>,
    addrlen_ptr: UserMut<u32>,
) -> KResult<()> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(EBADF)?;

    let local_addr = socket.local_addr().unwrap();
    if sockaddr_ptr.addr() != 0 {
        write_socket_addr(sockaddr_ptr, addrlen_ptr, local_addr)?;
    }

    Ok(())
}

#[eonix_macros::define_syscall(SYS_GETPEERNAME)]
async fn get_peername(
    sockfd: FD,
    sockaddr_ptr: UserMut<CSockAddr>,
    addrlen_ptr: UserMut<u32>,
) -> KResult<()> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let remote_addr = socket.remote_addr().ok_or(ENOTSOCK)?;
    if sockaddr_ptr.addr() != 0 {
        write_socket_addr(sockaddr_ptr, addrlen_ptr, remote_addr)?;
    }
    Ok(())
}

#[eonix_macros::define_syscall(SYS_BIND)]
async fn bind(sockfd: FD, sockaddr_ptr: User<CSockAddr>, addrlen: u32) -> KResult<()> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let socket_addr = read_socket_addr(sockaddr_ptr, addrlen as usize)?;

    println_debug!("bind socket {:?} to {:?}", sockfd, socket_addr);

    let res = socket.bind(socket_addr);
    res
}

#[eonix_macros::define_syscall(SYS_LISTEN)]
async fn listen(sockfd: FD, backlog: u32) -> KResult<()> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let res = socket.listen(backlog as usize);
    res
}

#[eonix_macros::define_syscall(SYS_ACCEPT)]
async fn accept(
    sockfd: FD,
    sockaddr_ptr: UserMut<CSockAddr>,
    addrlen_ptr: UserMut<u32>,
) -> KResult<FD> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let accepted_socket = socket.accept().await?;
    write_socket_addr(
        sockaddr_ptr,
        addrlen_ptr,
        accepted_socket.remote_addr().unwrap(),
    )?;
    let res = thread.files.socket(accepted_socket);
    res
}

#[eonix_macros::define_syscall(SYS_CONNECT)]
async fn connect(sockfd: FD, sockaddr_ptr: User<CSockAddr>, addrlen: u32) -> KResult<()> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let remote_addr = read_socket_addr(sockaddr_ptr, addrlen as usize)?;

    println_debug!("{:?}", remote_addr);

    let res = socket.connect(remote_addr).await;
    res
}

#[eonix_macros::define_syscall(SYS_RECVMSG)]
async fn recvmsg(sockfd: FD, msghdr_ptr: UserMut<MsgHdr>, flags: u32) -> KResult<usize> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let msghdr = UserPointer::new(msghdr_ptr.as_const())?.read()?;

    let mut iov_user = UserPointer::new(User::with_addr(msghdr.msg_iov))?;
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
            Ok(IoVec { base, len }) => {
                Some(UserBuffer::new(UserMut::with_addr(base.addr()), len.get()))
            }
        })
        .collect::<KResult<Vec<_>>>()?;

    let mut recv_metadata = None;
    let mut tot = 0usize;
    for mut buffer in iov_buffers.into_iter() {
        let (nread, recv_meta) = socket.recv(&mut buffer).await?;

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
        let addrlen_ptr = msghdr_ptr.cast::<usize>().offset(1).cast();
        write_socket_addr(
            UserMut::with_addr(msghdr.msg_name),
            addrlen_ptr,
            recv_metadata.unwrap().remote_addr,
        )?;
    }

    Ok(tot)
}

#[eonix_macros::define_syscall(SYS_RECVFROM)]
async fn recvfrom(
    sockfd: FD,
    buf: UserMut<u8>,
    len: usize,
    flags: u32,
    srcaddr_ptr: UserMut<CSockAddr>,
    addrlen_ptr: UserMut<u32>,
) -> KResult<usize> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let (ret, recv_meta) = socket.recv(&mut UserBuffer::new(buf, len)?).await?;

    if srcaddr_ptr.addr() != 0 {
        write_socket_addr(srcaddr_ptr, addrlen_ptr, recv_meta.remote_addr)?;
    }

    Ok(ret)
}

#[eonix_macros::define_syscall(SYS_SENDMSG)]
async fn sendmsg(sockfd: FD, msghdr: UserMut<MsgHdr>, flags: u32) -> KResult<usize> {
    let socket = thread
        .files
        .get(sockfd)
        .ok_or(EBADF)?
        .get_socket()?
        .ok_or(ENOTSOCK)?;

    let msghdr = UserPointer::new(msghdr.as_const())?.read()?;

    let mut iov_user = UserPointer::new(User::with_addr(msghdr.msg_iov))?;
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
                CheckedUserPointer::new(User::with_addr(base.addr()), len.get())
                    .map(|ptr| ptr.into_stream()),
            ),
        })
        .collect::<KResult<Vec<_>>>()?;

    let remote_addr = if msghdr.msg_namelen == 0 {
        None
    } else {
        Some(read_socket_addr(
            User::with_addr(msghdr.msg_name),
            msghdr.msg_namelen as usize,
        )?)
    };

    let mut tot = 0usize;
    for mut stream in iov_streams.into_iter() {
        let nread = socket
            .send(&mut stream, SendMetadata { remote_addr })
            .await?;
        tot += nread;

        if nread == 0 || !stream.is_drained() {
            break;
        }
    }

    Ok(tot)
}

#[eonix_macros::define_syscall(SYS_SENDTO)]
async fn sendto(
    sockfd: FD,
    buf: User<u8>,
    len: usize,
    _flags: u32,
    dstaddr_ptr: User<CSockAddr>,
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
    socket
        .send(&mut user_stream, SendMetadata { remote_addr })
        .await
}

pub fn keep_alive() {}
