use core::future::Future;
use core::net::SocketAddr;
use core::pin::Pin;
use core::task::{Poll, Waker};

use alloc::boxed::Box;
use alloc::sync::Arc;
use async_trait::async_trait;
use eonix_runtime::task::Task;
use eonix_sync::RwLock;
use smoltcp::socket::udp;
use smoltcp::wire::IpListenEndpoint;
use smoltcp::{iface::SocketHandle, wire::IpEndpoint};

use crate::io::{Buffer, Stream};
use crate::kernel::constants::{EADDRNOTAVAIL, EAGAIN, EINVAL};
use crate::net::iface::{get_ephemeral_iface, get_relate_iface, NetIface};
use crate::net::socket::{BoundSocket, RecvMetadata, SendMetadata, Socket, SocketType};
use crate::prelude::KResult;

pub struct UdpSocket {
    bound_socket: RwLock<Option<BoundSocket>>,
    local_addr: RwLock<Option<SocketAddr>>,
    is_nonblock: bool,
}

impl UdpSocket {
    pub fn new(is_nonblock: bool) -> Arc<Self> {
        Arc::new(Self {
            bound_socket: RwLock::new(None),
            local_addr: RwLock::new(None),
            is_nonblock,
        })
    }

    // Get single bound iface and socket handle
    fn iface_and_handle(&self) -> Option<(NetIface, SocketHandle)> {
        Task::block_on(self.bound_socket.read())
            .as_ref()?
            .as_single_bound()
            .map(|bound_socket| (bound_socket.iface(), bound_socket.handle()))
    }

    fn bind_impl<T>(iface: NetIface, socket_handle: SocketHandle, endpoint: T) -> KResult<()>
    where
        T: Into<IpListenEndpoint>,
    {
        let mut iface_guard = Task::block_on(iface.lock());

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
        let mut iface_guard = Task::block_on(iface.lock());

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
        let bound_socket_guard = Task::block_on(self.bound_socket.read());

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
        send_meta: SendMetadata,
        waker: &Waker,
    ) -> KResult<Option<usize>> {
        let mut iface_guard = Task::block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<udp::Socket>(socket_handle);

        match socket.send_with(
            stream.total(),
            IpEndpoint::from(send_meta.remote_addr.unwrap()),
            |tx_buffer| {
                stream
                    .poll_data(tx_buffer)
                    .unwrap()
                    .map(|data| data.len())
                    .unwrap_or(0)
            },
        ) {
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
        send_meta: SendMetadata,
        waker: &Waker,
    ) -> KResult<Option<usize>> {
        let remote_addr = send_meta.remote_addr.unwrap();
        let bound_socket_guard = Task::block_on(self.bound_socket.read());

        match bound_socket_guard.as_ref().unwrap() {
            BoundSocket::BoundAll(all) => {
                let iface = get_ephemeral_iface(Some(remote_addr.ip())).unwrap();
                let handle = all
                    .sockets
                    .iter()
                    .find(|item| Arc::ptr_eq(&item.iface, &iface))
                    .unwrap()
                    .handle();
                Self::send_impl(iface, handle, stream, send_meta, waker)
            }
            BoundSocket::BoundSingle(single) => {
                Self::send_impl(single.iface(), single.handle(), stream, send_meta, waker)
            }
        }
    }
}

#[async_trait]
impl Socket for UdpSocket {
    fn local_addr(&self) -> Option<SocketAddr> {
        Task::block_on(self.local_addr.read()).clone()
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        // FIXME: https://man7.org/linux/man-pages/man2/getpeername.2.html what we shoudl return for udp?
        None
    }

    fn bind(&self, socket_addr: SocketAddr) -> KResult<()> {
        let mut bound_socket_guard = Task::block_on(self.bound_socket.write());

        if bound_socket_guard.is_some() {
            return Err(EINVAL);
        }

        if socket_addr.ip().is_unspecified() {
            *bound_socket_guard = Some(BoundSocket::new_bind_all(
                socket_addr.port(),
                SocketType::Udp,
            )?);
            *Task::block_on(self.local_addr.write()) = Some(socket_addr);
        } else {
            let bind_iface = get_relate_iface(socket_addr.ip()).ok_or(EADDRNOTAVAIL)?;
            let (bound_socket, local_addr) =
                BoundSocket::new_bind_single(bind_iface, socket_addr.port(), SocketType::Udp)?;

            *bound_socket_guard = Some(bound_socket);
            *Task::block_on(self.local_addr.write()) = Some(local_addr);
        }
        drop(bound_socket_guard);

        let bound_socket_guard = Task::block_on(self.bound_socket.read());

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
        let remote_addr = send_meta.remote_addr.unwrap();
        let mut bound_socket_guard = Task::block_on(self.bound_socket.write());
        if bound_socket_guard.is_none() {
            let bind_iface = get_ephemeral_iface(Some(remote_addr.ip())).unwrap();
            let (bound_socket, local_addr) =
                BoundSocket::new_bind_single(bind_iface.clone(), 0, SocketType::Udp)?;
            let socket_handle = bound_socket.as_single_bound().unwrap().handle();
            Self::bind_impl(bind_iface, socket_handle, local_addr)?;
            *bound_socket_guard = Some(bound_socket);
            *Task::block_on(self.local_addr.write()) = Some(local_addr);
        }

        drop(bound_socket_guard);

        struct SendFuture<'a> {
            socket: &'a UdpSocket,
            send_meta: SendMetadata,
            stream: &'a mut dyn Stream,
        }

        impl<'a> Future for SendFuture<'a> {
            type Output = KResult<usize>;

            fn poll(self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> Poll<Self::Output> {
                let this = self.get_mut();
                match this
                    .socket
                    .try_send(this.stream, this.send_meta, cx.waker())
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
            send_meta,
            stream,
        }
        .await
    }
}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        let (iface, handle) = self.iface_and_handle().unwrap();

        let mut iface_guard = Task::block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<udp::Socket>(handle);

        socket.close();

        drop(iface_guard);
    }
}
