use crate::{
    io::{Buffer, Stream},
    net::iface::NetIface,
    prelude::KResult,
};
use alloc::{boxed::Box, sync::Arc};
use async_trait::async_trait;
use core::net::SocketAddr;
use eonix_runtime::task::Task;
use smoltcp::iface::SocketHandle;

pub mod tcp;

#[async_trait]
pub trait Socket: Sync + Send {
    fn bind(&self, _socket_addr: SocketAddr) -> KResult<()> {
        panic!("Unimplemented");
    }

    fn listen(&self, _backlog: usize) -> KResult<()> {
        panic!("Unimplemented");
    }

    async fn connect(&self, _remote_addr: SocketAddr) -> KResult<()> {
        panic!("Unimplemented");
    }

    async fn accept(&self) -> KResult<(Arc<dyn Socket>, SocketAddr)> {
        panic!("Unimplemented");
    }

    async fn recv(&self, buffer: &mut dyn Buffer) -> KResult<usize>;

    async fn send(&self, stream: &mut dyn Stream) -> KResult<usize>;

    fn close(&self) -> KResult<()> {
        panic!("Unimplemented");
    }
}

struct BoundSocket {
    iface: NetIface,
    socket_addr: SocketAddr,
    socket_handle: SocketHandle,
}

impl BoundSocket {
    pub fn new_bind(iface: NetIface, bind_port: Option<u16>) -> KResult<Self> {
        let (socket_addr, socket_handle) = {
            let mut iface_guard = Task::block_on(iface.lock());

            iface_guard.bind_tcp_socket(bind_port.unwrap_or(0))?
        };

        Ok(Self {
            iface,
            socket_addr,
            socket_handle,
        })
    }

    pub fn new_accept(
        iface: NetIface,
        socket_handle: SocketHandle,
        socket_addr: SocketAddr,
    ) -> Self {
        Self {
            iface,
            socket_addr,
            socket_handle,
        }
    }

    pub fn iface(&self) -> NetIface {
        self.iface.clone()
    }

    pub fn handle(&self) -> SocketHandle {
        self.socket_handle
    }

    pub fn socket_addr(&self) -> SocketAddr {
        self.socket_addr
    }
}
