use crate::{
    io::{Buffer, Stream},
    kernel::{
        constants::{EADDRINUSE, EINVAL},
        vfs::file::PollEvent,
    },
    net::iface::{NetIface, IFACES},
    prelude::KResult,
};
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use async_trait::async_trait;
use core::net::SocketAddr;
use eonix_runtime::task::Task;
use smoltcp::iface::SocketHandle;

pub mod tcp;
pub mod udp;

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum SocketType {
    Tcp,
    Udp,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SendMetadata {
    pub remote_addr: Option<SocketAddr>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecvMetadata {
    pub remote_addr: SocketAddr,
}

#[async_trait]
pub trait Socket: Sync + Send {
    fn bind(&self, _socket_addr: SocketAddr) -> KResult<()> {
        Err(EINVAL)
    }

    fn listen(&self, _backlog: usize) -> KResult<()> {
        Err(EINVAL)
    }

    async fn connect(&self, _remote_addr: SocketAddr) -> KResult<()> {
        Err(EINVAL)
    }

    async fn accept(&self) -> KResult<Arc<dyn Socket>> {
        Err(EINVAL)
    }

    fn local_addr(&self) -> Option<SocketAddr>;

    fn remote_addr(&self) -> Option<SocketAddr>;

    async fn recv(&self, buffer: &mut dyn Buffer) -> KResult<(usize, RecvMetadata)>;

    async fn send(&self, stream: &mut dyn Stream, send_meta: SendMetadata) -> KResult<usize>;

    fn poll(&self, events: PollEvent) -> KResult<PollEvent>;
}

#[derive(Clone)]
pub enum BoundSocket {
    BoundSingle(BoundSingle),
    BoundAll(BoundAll),
}

impl BoundSocket {
    fn new_bind_all(bind_port: u16, socket_type: SocketType) -> KResult<Self> {
        Ok(BoundSocket::BoundAll(BoundAll::new_bind(
            bind_port,
            socket_type,
        )?))
    }

    fn new_bind_single(
        iface: NetIface,
        bind_port: u16,
        socket_type: SocketType,
    ) -> KResult<(Self, SocketAddr)> {
        let (single, sock_addr) = BoundSingle::new_bind(iface, bind_port, socket_type)?;
        Ok((BoundSocket::BoundSingle(single), sock_addr))
    }

    fn new_single(iface: NetIface, socket_handle: SocketHandle) -> KResult<Self> {
        Ok(BoundSocket::BoundSingle(BoundSingle::new(
            iface,
            socket_handle,
        )))
    }

    fn as_single_bound(&self) -> Option<&BoundSingle> {
        match self {
            BoundSocket::BoundSingle(single) => Some(single),
            _ => None,
        }
    }

    fn as_all_bound(&self) -> Option<&BoundAll> {
        match self {
            BoundSocket::BoundAll(all) => Some(all),
            _ => None,
        }
    }
}

/// BoundAll is only used for socket listen all ifaces
#[derive(Clone)]
struct BoundAll {
    port: u16,
    // FIXME: need support IFACES dyn change
    sockets: Vec<BoundSingle>,
}

impl BoundAll {
    fn new_bind(bind_port: u16, socket_type: SocketType) -> KResult<Self> {
        let ifaces_guard = Task::block_on(IFACES.lock());

        let mut sockets = Vec::new();
        for iface in ifaces_guard.values() {
            // FIXME: handle err except eaddrinuse
            if let Ok((item, _)) = BoundSingle::new_bind(iface.clone(), bind_port, socket_type) {
                sockets.push(item);
            }
        }

        if sockets.len() == 0 {
            return Err(EADDRINUSE);
        }

        Ok(Self {
            port: bind_port,
            sockets,
        })
    }
}

#[derive(Clone)]
struct BoundSingle {
    iface: NetIface,
    socket_handle: SocketHandle,
}

impl BoundSingle {
    fn new(iface: NetIface, socket_handle: SocketHandle) -> Self {
        Self {
            iface,
            socket_handle,
        }
    }

    fn new_bind(
        iface: NetIface,
        bind_port: u16,
        socket_type: SocketType,
    ) -> KResult<(Self, SocketAddr)> {
        let (socket_addr, socket_handle) = {
            let mut iface_guard = Task::block_on(iface.lock());

            iface_guard.bind_socket(bind_port, socket_type)?
        };

        Ok((
            Self {
                iface,
                socket_handle,
            },
            socket_addr,
        ))
    }

    fn iface(&self) -> NetIface {
        self.iface.clone()
    }

    fn handle(&self) -> SocketHandle {
        self.socket_handle
    }
}
