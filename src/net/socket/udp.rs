use core::future::Future;
use core::net::SocketAddr;
use core::pin::Pin;
use core::task::{Poll, Waker};

use alloc::boxed::Box;
use alloc::collections::btree_map::BTreeMap;
use alloc::sync::Arc;
use async_trait::async_trait;
use eonix_sync::{RwLock, Spin};
use smoltcp::socket::udp;
use smoltcp::wire::IpListenEndpoint;
use smoltcp::{iface::SocketHandle, wire::IpEndpoint};

use crate::io::{Buffer, Stream};
use crate::kernel::constants::{EADDRNOTAVAIL, EAGAIN, EINVAL};
use crate::kernel::task::block_on;
use crate::kernel::vfs::PollEvent;
use crate::net::iface::{get_ephemeral_iface, get_relate_iface, NetIface};
use crate::net::socket::{BoundSocket, RecvMetadata, SendMetadata, Socket, SocketType};
use crate::prelude::KResult;

// FIXME:
pub static UDP_PORT_MAP: Spin<BTreeMap<SocketAddr, BoundSocket>> = Spin::new(BTreeMap::new());

pub struct UdpSocket {
    bound_socket: RwLock<Option<BoundSocket>>,
    local_addr: RwLock<Option<SocketAddr>>,
    remote_addr: RwLock<Option<SocketAddr>>,
    is_nonblock: bool,
    // FIXME: can ensure the order
    is_reuse_other: Spin<bool>,
}

impl UdpSocket {
    pub fn new(is_nonblock: bool) -> Arc<Self> {
        Arc::new(Self {
            bound_socket: RwLock::new(None),
            local_addr: RwLock::new(None),
            remote_addr: RwLock::new(None),
            is_nonblock,
            is_reuse_other: Spin::new(false),
        })
    }

    // Get single bound iface and socket handle
    fn iface_and_handle(&self) -> Option<(NetIface, SocketHandle)> {
        block_on(self.bound_socket.read())
            .as_ref()?
            .as_single_bound()
            .map(|bound_socket| (bound_socket.iface(), bound_socket.handle()))
    }

    fn bind_impl<T>(iface: NetIface, socket_handle: SocketHandle, endpoint: T) -> KResult<()>
    where
        T: Into<IpListenEndpoint>,
    {
        let mut iface_guard = block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<udp::Socket>(socket_handle);

        let _ = socket.bind(endpoint);
        Ok(())
    }

    fn recv_impl(
        iface: NetIface,
        socket_handle: SocketHandle,
        buf: &mut dyn Buffer,
        waker: &Waker,
    ) -> KResult<Option<(usize, RecvMetadata)>> {
        let mut iface_guard = block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<udp::Socket>(socket_handle);

        match socket.recv() {
            Ok((data, meta)) => {
                buf.fill(data)?;
                Ok(Some((
                    buf.wrote(),
                    RecvMetadata {
                        remote_addr: meta.endpoint.into(),
                    },
                )))
            }
            Err(udp::RecvError::Exhausted) => {
                socket.register_recv_waker(waker);
                Ok(None)
            }
            Err(udp::RecvError::Truncated) => Err(EINVAL),
        }
    }

    fn try_recv(
        &self,
        buf: &mut dyn Buffer,
        waker: &Waker,
    ) -> KResult<Option<(usize, RecvMetadata)>> {
        let bound_socket_guard = block_on(self.bound_socket.read());

        match bound_socket_guard.as_ref().unwrap() {
            BoundSocket::BoundAll(all) => {
                for item in &all.sockets {
                    let ret = Self::recv_impl(item.iface(), item.handle(), buf, waker)?;
                    if ret.is_some() {
                        return Ok(ret);
                    }
                }
                Ok(None)
            }
            BoundSocket::BoundSingle(single) => {
                Self::recv_impl(single.iface(), single.handle(), buf, waker)
            }
        }
    }

    fn send_impl(
        iface: NetIface,
        socket_handle: SocketHandle,
        stream: &mut dyn Stream,
        remote_addr: SocketAddr,
        waker: &Waker,
    ) -> KResult<Option<usize>> {
        let mut iface_guard = block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<udp::Socket>(socket_handle);

        match socket.send_with(stream.total(), IpEndpoint::from(remote_addr), |tx_buffer| {
            stream
                .poll_data(tx_buffer)
                .unwrap()
                .map(|data| data.len())
                .unwrap_or(0)
        }) {
            Ok(res) => Ok(Some(res)),
            Err(udp::SendError::BufferFull) => {
                socket.register_send_waker(waker);
                Ok(None)
            }
            Err(udp::SendError::Unaddressable) => Err(EINVAL),
        }
    }

    fn try_send(
        &self,
        stream: &mut dyn Stream,
        remote_addr: SocketAddr,
        waker: &Waker,
    ) -> KResult<Option<usize>> {
        let bound_socket_guard = block_on(self.bound_socket.read());

        match bound_socket_guard.as_ref().unwrap() {
            BoundSocket::BoundAll(all) => {
                let iface = get_ephemeral_iface(Some(remote_addr.ip())).unwrap();
                let handle = all
                    .sockets
                    .iter()
                    .find(|item| Arc::ptr_eq(&item.iface, &iface))
                    .unwrap()
                    .handle();
                Self::send_impl(iface, handle, stream, remote_addr, waker)
            }
            BoundSocket::BoundSingle(single) => {
                Self::send_impl(single.iface(), single.handle(), stream, remote_addr, waker)
            }
        }
    }

    fn poll_impl(
        iface: NetIface,
        socket_handle: SocketHandle,
        events: PollEvent,
    ) -> KResult<PollEvent> {
        let mut iface_guard = block_on(iface.lock());
        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<udp::Socket>(socket_handle);

        let mut poll_state = PollEvent::empty();
        if events.contains(PollEvent::Readable) {
            if socket.can_recv() {
                poll_state |= PollEvent::Readable;
            }
        }
        if events.contains(PollEvent::Writable) {
            if socket.can_send() {
                poll_state |= PollEvent::Writable;
            }
        }
        Ok(poll_state)
    }
}

#[async_trait]
impl Socket for UdpSocket {
    fn local_addr(&self) -> Option<SocketAddr> {
        block_on(self.local_addr.read()).clone()
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        block_on(self.remote_addr.read()).clone()
    }

    fn bind(&self, socket_addr: SocketAddr) -> KResult<()> {
        let mut bound_socket_guard = block_on(self.bound_socket.write());

        if bound_socket_guard.is_some() {
            return Err(EINVAL);
        }

        *block_on(self.local_addr.write()) = Some(socket_addr);

        if let Some(bound_sock) = UDP_PORT_MAP.lock().get(&socket_addr) {
            *bound_socket_guard = Some(bound_sock.clone());
            *self.is_reuse_other.lock() = true;
            return Ok(());
        }

        let (bound_socket, local_addr) = if socket_addr.ip().is_unspecified() {
            (
                BoundSocket::new_bind_all(socket_addr.port(), SocketType::Udp)?,
                socket_addr,
            )
        } else {
            let bind_iface = get_relate_iface(socket_addr.ip()).ok_or(EADDRNOTAVAIL)?;
            BoundSocket::new_bind_single(bind_iface, socket_addr.port(), SocketType::Udp)?
        };

        *block_on(self.local_addr.write()) = Some(local_addr);
        UDP_PORT_MAP.lock().insert(local_addr, bound_socket.clone());
        *bound_socket_guard = Some(bound_socket);
        drop(bound_socket_guard);

        let bound_socket_guard = block_on(self.bound_socket.read());

        match bound_socket_guard.as_ref().unwrap() {
            BoundSocket::BoundAll(all) => {
                for item in &all.sockets {
                    Self::bind_impl(item.iface(), item.handle(), socket_addr.port())?
                }
            }
            BoundSocket::BoundSingle(single) => {
                Self::bind_impl(single.iface(), single.handle(), socket_addr)?
            }
        }

        Ok(())
    }

    async fn connect(&self, remote_addr: SocketAddr) -> KResult<()> {
        *(self.remote_addr.write().await) = Some(remote_addr);

        Ok(())
    }

    async fn recv(&self, buffer: &mut dyn Buffer) -> KResult<(usize, RecvMetadata)> {
        struct RecvFuture<'a> {
            socket: &'a UdpSocket,
            buffer: &'a mut dyn Buffer,
        }

        impl<'a> Future for RecvFuture<'a> {
            type Output = KResult<(usize, RecvMetadata)>;

            fn poll(self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> Poll<Self::Output> {
                let this = self.get_mut();
                match this.socket.try_recv(this.buffer, cx.waker()) {
                    Ok(Some(res)) => Poll::Ready(Ok(res)),
                    Ok(None) if this.socket.is_nonblock => Poll::Ready(Err(EAGAIN)),
                    Ok(None) => Poll::Pending,
                    Err(err) => Poll::Ready(Err(err)),
                }
            }
        }

        RecvFuture {
            socket: self,
            buffer,
        }
        .await
    }

    async fn send(&self, stream: &mut dyn Stream, send_meta: SendMetadata) -> KResult<usize> {
        let remote_addr = if let Some(remote_addr) = send_meta.remote_addr {
            remote_addr
        } else {
            block_on(self.remote_addr.read()).clone().unwrap()
        };

        let mut bound_socket_guard = block_on(self.bound_socket.write());
        if bound_socket_guard.is_none() {
            let bind_iface = get_ephemeral_iface(Some(remote_addr.ip())).unwrap();
            let (bound_socket, local_addr) =
                BoundSocket::new_bind_single(bind_iface.clone(), 0, SocketType::Udp)?;
            let socket_handle = bound_socket.as_single_bound().unwrap().handle();
            Self::bind_impl(bind_iface, socket_handle, local_addr)?;
            *bound_socket_guard = Some(bound_socket);
            *block_on(self.local_addr.write()) = Some(local_addr);
        }

        drop(bound_socket_guard);

        struct SendFuture<'a> {
            socket: &'a UdpSocket,
            remote_addr: SocketAddr,
            stream: &'a mut dyn Stream,
        }

        impl<'a> Future for SendFuture<'a> {
            type Output = KResult<usize>;

            fn poll(self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> Poll<Self::Output> {
                let this = self.get_mut();
                match this
                    .socket
                    .try_send(this.stream, this.remote_addr, cx.waker())
                {
                    Ok(Some(res)) => Poll::Ready(Ok(res)),
                    Ok(None) if this.socket.is_nonblock => Poll::Ready(Err(EAGAIN)),
                    Ok(None) => Poll::Pending,
                    Err(err) => Poll::Ready(Err(err)),
                }
            }
        }

        SendFuture {
            socket: self,
            remote_addr,
            stream,
        }
        .await
    }

    fn poll(&self, events: PollEvent) -> KResult<PollEvent> {
        let bound_socket_guard = block_on(self.bound_socket.read());

        match bound_socket_guard.as_ref().unwrap() {
            BoundSocket::BoundSingle(single) => {
                Self::poll_impl(single.iface(), single.handle(), events)
            }
            BoundSocket::BoundAll(all) => {
                let mut poll_state = PollEvent::empty();
                for bound_socket in &all.sockets {
                    poll_state |=
                        Self::poll_impl(bound_socket.iface(), bound_socket.handle(), events)?
                }
                Ok(poll_state)
            }
        }
    }
}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        let bound_socket_guard = block_on(self.bound_socket.read());

        if bound_socket_guard.is_none() {
            return;
        }

        if *self.is_reuse_other.lock() {
            return;
        }

        let local_addr = self.local_addr().unwrap();
        let port = local_addr.port();

        UDP_PORT_MAP.lock().remove(&local_addr);
        match bound_socket_guard.as_ref().unwrap() {
            BoundSocket::BoundAll(all) => {
                for item in &all.sockets {
                    close_impl(item.iface(), item.handle(), port);
                }
            }
            BoundSocket::BoundSingle(single) => close_impl(single.iface(), single.handle(), port),
        }

        fn close_impl(iface: NetIface, handle: SocketHandle, port: u16) {
            let mut iface_guard = block_on(iface.lock());

            let socket = iface_guard
                .iface_and_sockets()
                .1
                .get_mut::<udp::Socket>(handle);

            socket.close();

            iface_guard.poll();

            iface_guard.remove_socket(handle, port, SocketType::Udp);
        }
    }
}
