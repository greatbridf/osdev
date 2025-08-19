use core::future::Future;
use core::net::SocketAddr;
use core::pin::Pin;
use core::task::{Poll, Waker};

use alloc::boxed::Box;
use alloc::sync::Arc;
use async_trait::async_trait;
use eonix_sync::RwLock;
use smoltcp::socket::tcp;
use smoltcp::wire::IpListenEndpoint;
use smoltcp::{iface::SocketHandle, wire::IpEndpoint};

use crate::io::{Buffer, Stream};
use crate::kernel::constants::{EADDRNOTAVAIL, EAGAIN, ECONNREFUSED, EINVAL, EISCONN, ENOTCONN};
use crate::kernel::task::block_on;
use crate::kernel::vfs::PollEvent;
use crate::net::iface::{get_ephemeral_iface, get_relate_iface, NetIface};
use crate::net::socket::{BoundSocket, RecvMetadata, SendMetadata, Socket, SocketType};
use crate::prelude::KResult;

pub struct TcpSocket {
    bound_socket: RwLock<Option<BoundSocket>>,
    local_addr: RwLock<Option<SocketAddr>>,
    remote_addr: RwLock<Option<SocketAddr>>,
    is_nonblock: bool,
}

impl TcpSocket {
    pub fn new(is_nonblock: bool) -> Arc<Self> {
        Arc::new(Self {
            bound_socket: RwLock::new(None),
            local_addr: RwLock::new(None),
            remote_addr: RwLock::new(None),
            is_nonblock,
        })
    }

    // Get single bound iface and socket handle
    fn iface_and_handle(&self) -> Option<(NetIface, SocketHandle)> {
        block_on(self.bound_socket.read())
            .as_ref()?
            .as_single_bound()
            .map(|bound_socket| (bound_socket.iface(), bound_socket.handle()))
    }

    fn listen_impl<T>(
        iface: NetIface,
        socket_handle: SocketHandle,
        local_endpoint: T,
        backlog: usize,
    ) -> KResult<()>
    where
        T: Into<IpListenEndpoint>,
    {
        let mut iface_guard = block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<tcp::Socket>(socket_handle);

        socket
            .listen(local_endpoint, Some(backlog))
            .map_err(|_| EINVAL)
    }

    fn accept_impl(
        iface: NetIface,
        socket_handle: SocketHandle,
        waker: &Waker,
    ) -> KResult<Option<Arc<TcpSocket>>> {
        let mut iface_guard = block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<tcp::Socket>(socket_handle);

        if socket.may_accept() {
            match socket.accept() {
                Ok(Some(new_socket)) => {
                    let socket_addr = SocketAddr::from(new_socket.local_endpoint().unwrap());
                    let remote_addr = SocketAddr::from(new_socket.remote_endpoint().unwrap());
                    let handle = iface_guard.iface_and_sockets().1.add(new_socket);
                    let accept_socket = Arc::new(TcpSocket {
                        bound_socket: RwLock::new(Some(BoundSocket::new_single(
                            iface.clone(),
                            handle,
                        )?)),
                        local_addr: RwLock::new(Some(socket_addr)),
                        remote_addr: RwLock::new(Some(remote_addr)),
                        is_nonblock: false,
                    });
                    Ok(Some(accept_socket))
                }
                Ok(None) => {
                    socket.register_recv_waker(waker);
                    Ok(None)
                }
                Err(_) => Err(EINVAL),
            }
        } else {
            Err(EINVAL)
        }
    }

    fn try_accept(&self, waker: &Waker) -> KResult<Option<Arc<TcpSocket>>> {
        let bound_socket_guard = block_on(self.bound_socket.read());

        match bound_socket_guard.as_ref().unwrap() {
            BoundSocket::BoundSingle(single) => {
                Self::accept_impl(single.iface(), single.handle(), waker)
            }
            BoundSocket::BoundAll(all) => {
                for bound_socket in &all.sockets {
                    let res =
                        Self::accept_impl(bound_socket.iface(), bound_socket.handle(), waker)?;
                    if res.is_some() {
                        return Ok(res);
                    }
                }
                Ok(None)
            }
        }
    }

    fn try_recv(
        &self,
        buf: &mut dyn Buffer,
        waker: &Waker,
    ) -> KResult<Option<(usize, RecvMetadata)>> {
        let (iface, handle) = self.iface_and_handle().unwrap();

        let mut iface_guard = block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<tcp::Socket>(handle);

        if !socket.may_recv() {
            Err(ENOTCONN)
        } else if socket.can_recv() {
            let len = socket
                .recv(|rx_data| {
                    let _ = buf.fill(&rx_data[..]);
                    (buf.wrote(), buf.wrote())
                })
                .unwrap(); // unwrap() is safe due to the source code logic
            Ok(Some((
                len,
                RecvMetadata {
                    remote_addr: self.remote_addr().unwrap(),
                },
            )))
        } else {
            socket.register_recv_waker(waker);
            Ok(None)
        }
    }

    fn try_send(&self, stream: &mut dyn Stream, waker: &Waker) -> KResult<Option<usize>> {
        let (iface, handle) = self.iface_and_handle().unwrap();

        let mut iface_guard = block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<tcp::Socket>(handle);

        if !socket.may_send() {
            Err(ENOTCONN)
        } else if socket.can_send() {
            let result = socket
                .send(|tx_data| {
                    let result = stream
                        .poll_data(tx_data)
                        .map(|data| data.map(|write_in| write_in.len()).unwrap_or(0));
                    (result.unwrap_or(0), result)
                })
                .unwrap(); // unwrap() is safe due to the source code logic

            result.map(|len| Some(len))
        } else {
            socket.register_send_waker(waker);
            Ok(None)
        }
    }

    fn check_connect(&self, waker: &Waker) -> KResult<Option<()>> {
        let (iface, handle) = self.iface_and_handle().unwrap();

        let mut iface_guard = block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<tcp::Socket>(handle);

        if socket.state() == tcp::State::Established {
            Ok(Some(()))
        } else if !socket.is_active() {
            Err(ECONNREFUSED)
        } else {
            socket.register_recv_waker(waker);
            Ok(None)
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
            .get_mut::<tcp::Socket>(socket_handle);

        match socket.state() {
            tcp::State::Established => {
                let mut poll_state = PollEvent::empty();
                if events.contains(PollEvent::Readable) {
                    if !socket.may_recv() || socket.can_recv() {
                        poll_state |= PollEvent::Readable;
                    }
                }

                if events.contains(PollEvent::Writable) {
                    if !socket.may_send() || socket.can_send() {
                        poll_state |= PollEvent::Writable;
                    }
                }
                Ok(poll_state)
            }
            tcp::State::Listen => {
                let mut poll_state = PollEvent::empty();
                if events.contains(PollEvent::Readable) {
                    if socket.can_accept() {
                        poll_state |= PollEvent::Readable;
                    }
                }
                Ok(poll_state)
            }
            _ => Ok(events),
        }
    }
}

#[async_trait]
impl Socket for TcpSocket {
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

        if socket_addr.ip().is_unspecified() {
            *bound_socket_guard = Some(BoundSocket::new_bind_all(
                socket_addr.port(),
                SocketType::Tcp,
            )?);
            *block_on(self.local_addr.write()) = Some(socket_addr);
        } else {
            let bind_iface = get_relate_iface(socket_addr.ip()).ok_or(EADDRNOTAVAIL)?;

            let (bound_socket, local_addr) =
                BoundSocket::new_bind_single(bind_iface, socket_addr.port(), SocketType::Tcp)?;

            *bound_socket_guard = Some(bound_socket);
            *block_on(self.local_addr.write()) = Some(local_addr);
        }
        drop(bound_socket_guard);

        Ok(())
    }

    fn listen(&self, backlog: usize) -> KResult<()> {
        let bound_socket_guard = block_on(self.bound_socket.write());

        if bound_socket_guard.is_none() {
            // FIXME: should get a available port and bind to all iface
            // need a way to get that port simply
            // *bound_socket_guard = Some(BoundSocket::new_bind_all(available_port)?);
            unimplemented!()
        }

        drop(bound_socket_guard);

        let local_addr = self.local_addr().unwrap();

        let bound_socket_guard = block_on(self.bound_socket.read());

        match bound_socket_guard.as_ref().unwrap() {
            BoundSocket::BoundAll(all) => {
                for item in &all.sockets {
                    let (iface, handle) = (item.iface(), item.handle());
                    Self::listen_impl(iface, handle, local_addr.port(), backlog)?;
                }
                Ok(())
            }
            BoundSocket::BoundSingle(single) => {
                let (iface, handle) = (single.iface(), single.handle());
                Self::listen_impl(iface, handle, local_addr, backlog)
            }
        }
    }

    async fn connect(&self, remote_addr: SocketAddr) -> KResult<()> {
        let mut bound_socket_guard = self.bound_socket.write().await;

        if bound_socket_guard.is_none() {
            let bind_iface = get_ephemeral_iface(Some(remote_addr.ip()));
            let (bound_socket, local_addr) =
                BoundSocket::new_bind_single(bind_iface.unwrap(), 0, SocketType::Tcp)?;
            *bound_socket_guard = Some(bound_socket);
            *block_on(self.local_addr.write()) = Some(local_addr);
        }

        drop(bound_socket_guard);

        let (iface, handle) = self.iface_and_handle().unwrap();

        let mut iface_guard = iface.lock().await;
        let (iface_inner, sockets) = iface_guard.iface_and_sockets();
        let socket = sockets.get_mut::<tcp::Socket>(handle);

        socket
            .connect(
                iface_inner.context(),
                IpEndpoint::from(remote_addr),
                IpEndpoint::from(self.local_addr().unwrap()),
            )
            .map_err(|err| match err {
                tcp::ConnectError::Unaddressable => EADDRNOTAVAIL,
                // FIXME: Should return EISCONN ?
                tcp::ConnectError::InvalidState => EISCONN,
            })?;

        drop(iface_guard);

        *block_on(self.remote_addr.write()) = Some(remote_addr);

        struct ConnectFuture<'a> {
            socket: &'a TcpSocket,
        }

        impl<'a> Future for ConnectFuture<'a> {
            type Output = KResult<()>;

            fn poll(self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> Poll<Self::Output> {
                let this = self.get_mut();
                match this.socket.check_connect(cx.waker()) {
                    Ok(Some(_)) => Poll::Ready(Ok(())),
                    Ok(None) if this.socket.is_nonblock => Poll::Ready(Err(EAGAIN)),
                    Ok(None) => Poll::Pending,
                    Err(err) => Poll::Ready(Err(err)),
                }
            }
        }

        ConnectFuture { socket: self }.await
    }

    async fn accept(&self) -> KResult<Arc<dyn Socket>> {
        struct AcceptFuture<'a> {
            socket: &'a TcpSocket,
        }

        impl<'a> Future for AcceptFuture<'a> {
            type Output = KResult<Arc<dyn Socket>>;

            fn poll(self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> Poll<Self::Output> {
                let this = self.get_mut();
                match this.socket.try_accept(cx.waker()) {
                    Ok(Some(res)) => Poll::Ready(Ok(res)),
                    Ok(None) if this.socket.is_nonblock => Poll::Ready(Err(EAGAIN)),
                    Ok(None) => Poll::Pending,
                    Err(err) => Poll::Ready(Err(err)),
                }
            }
        }

        AcceptFuture { socket: self }.await
    }

    async fn recv(&self, buffer: &mut dyn Buffer) -> KResult<(usize, RecvMetadata)> {
        struct RecvFuture<'a> {
            socket: &'a TcpSocket,
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

    async fn send(&self, stream: &mut dyn Stream, _send_meta: SendMetadata) -> KResult<usize> {
        struct SendFuture<'a> {
            socket: &'a TcpSocket,
            stream: &'a mut dyn Stream,
        }

        impl<'a> Future for SendFuture<'a> {
            type Output = KResult<usize>;

            fn poll(self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> Poll<Self::Output> {
                let this = self.get_mut();
                match this.socket.try_send(this.stream, cx.waker()) {
                    Ok(Some(res)) => Poll::Ready(Ok(res)),
                    Ok(None) if this.socket.is_nonblock => Poll::Ready(Err(EAGAIN)),
                    Ok(None) => Poll::Pending,
                    Err(err) => Poll::Ready(Err(err)),
                }
            }
        }

        SendFuture {
            socket: self,
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
                    poll_state |= Self::poll_impl(
                        bound_socket.iface(),
                        bound_socket.handle(),
                        events.clone(),
                    )?
                }
                Ok(poll_state)
            }
        }
    }
}

impl Drop for TcpSocket {
    fn drop(&mut self) {
        let bound_socket_guard = block_on(self.bound_socket.read());

        if bound_socket_guard.is_none() {
            return;
        }

        let port = self.local_addr().unwrap().port();

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
                .get_mut::<tcp::Socket>(handle);

            socket.close();

            iface_guard.poll();

            iface_guard.remove_socket(handle, port, SocketType::Tcp);
        }
    }
}
