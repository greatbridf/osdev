use core::future::Future;
use core::net::SocketAddr;
use core::pin::Pin;
use core::task::{Poll, Waker};

use alloc::boxed::Box;
use alloc::sync::Arc;
use async_trait::async_trait;
use eonix_runtime::task::Task;
use eonix_sync::Mutex;
use smoltcp::socket::tcp;
use smoltcp::{iface::SocketHandle, wire::IpEndpoint};

use crate::io::{Buffer, Stream};
use crate::kernel::constants::{EADDRNOTAVAIL, EAGAIN, ECONNREFUSED, EINVAL, EISCONN, ENOTCONN};
use crate::net::iface::{get_ephemeral_iface, get_relate_iface, NetIface};
use crate::net::socket::{BoundSocket, Socket};
use crate::prelude::KResult;

pub struct TcpSocket {
    bound_socket: Mutex<Option<BoundSocket>>,
    is_nonblock: bool,
}

impl TcpSocket {
    pub fn new(is_nonblock: bool) -> Arc<Self> {
        Arc::new(Self {
            bound_socket: Mutex::new(None),
            is_nonblock,
        })
    }

    fn iface(&self) -> Option<NetIface> {
        Task::block_on(self.bound_socket.lock())
            .as_ref()
            .map(|bound_socket| bound_socket.iface())
    }

    pub fn handle(&self) -> Option<SocketHandle> {
        Task::block_on(self.bound_socket.lock())
            .as_ref()
            .map(|bound_socket| bound_socket.handle())
    }

    pub fn local_addr(&self) -> Option<SocketAddr> {
        Task::block_on(self.bound_socket.lock())
            .as_ref()
            .map(|bound_socket| bound_socket.socket_addr())
    }

    fn try_accept(&self, waker: &Waker) -> Option<KResult<(Arc<TcpSocket>, SocketAddr)>> {
        let iface = self.iface().unwrap();

        let mut iface_guard = Task::block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<tcp::Socket>(self.handle().unwrap());

        if socket.may_accept() {
            match socket.accept() {
                Ok(Some(new_socket)) => {
                    let socket_addr = SocketAddr::from(new_socket.local_endpoint().unwrap());
                    let remote_addr = SocketAddr::from(new_socket.remote_endpoint().unwrap());
                    let handle = iface_guard.iface_and_sockets().1.add(new_socket);
                    let accept_socket = Arc::new(TcpSocket {
                        bound_socket: Mutex::new(Some(BoundSocket::new_accept(
                            iface.clone(),
                            handle,
                            socket_addr,
                        ))),
                        is_nonblock: false,
                    });
                    Some(Ok((accept_socket, remote_addr)))
                }
                Ok(None) => {
                    socket.register_recv_waker(waker);
                    None
                }
                Err(_) => Some(Err(EINVAL)),
            }
        } else {
            Some(Err(EINVAL))
        }
    }

    fn try_recv(&self, buf: &mut dyn Buffer, waker: &Waker) -> Option<KResult<usize>> {
        let iface = self.iface().unwrap();

        let mut iface_guard = Task::block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<tcp::Socket>(self.handle().unwrap());

        if !socket.may_recv() {
            Some(Err(ENOTCONN))
        } else if socket.can_recv() {
            let len = socket
                .recv(|rx_data| {
                    let _ = buf.fill(&rx_data[..]);
                    (buf.wrote(), buf.wrote())
                })
                .unwrap(); // unwrap() is safe due to the source code logic
            Some(Ok(len))
        } else {
            socket.register_recv_waker(waker);
            None
        }
    }

    fn try_send(&self, stream: &mut dyn Stream, waker: &Waker) -> Option<KResult<usize>> {
        let iface = self.iface().unwrap();

        let mut iface_guard = Task::block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<tcp::Socket>(self.handle().unwrap());

        if !socket.may_send() {
            Some(Err(ENOTCONN))
        } else if socket.can_send() {
            let result = socket
                .send(|tx_data| {
                    let result = stream
                        .poll_data(tx_data)
                        .map(|data| data.map(|write_in| write_in.len()).unwrap_or(0));
                    (result.unwrap_or(0), result)
                })
                .unwrap(); // unwrap() is safe due to the source code logic

            Some(result)
        } else {
            socket.register_send_waker(waker);
            None
        }
    }

    fn check_connect(&self, waker: &Waker) -> Option<KResult<()>> {
        let iface = self.iface().unwrap();

        let mut iface_guard = Task::block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<tcp::Socket>(self.handle().unwrap());

        if socket.state() == tcp::State::Established {
            Some(Ok(()))
        } else if !socket.is_active() {
            Some(Err(ECONNREFUSED))
        } else {
            socket.register_recv_waker(waker);
            None
        }
    }
}

#[async_trait]
impl Socket for TcpSocket {
    fn bind(&self, socket_addr: SocketAddr) -> KResult<()> {
        let mut bound_socket_guard = Task::block_on(self.bound_socket.lock());

        if bound_socket_guard.is_some() {
            return Err(EINVAL);
        }

        let bind_iface = get_relate_iface(socket_addr.ip()).ok_or(EADDRNOTAVAIL)?;

        *bound_socket_guard = Some(BoundSocket::new_bind(bind_iface, Some(socket_addr.port()))?);

        Ok(())
    }

    fn listen(&self, backlog: usize) -> KResult<()> {
        let mut bound_socket_guard = Task::block_on(self.bound_socket.lock());

        if bound_socket_guard.is_none() {
            let bind_iface = get_ephemeral_iface(None);
            *bound_socket_guard = Some(BoundSocket::new_bind(bind_iface.unwrap(), None)?);
        }

        drop(bound_socket_guard);

        let iface = self.iface().unwrap();
        let mut iface_guard = Task::block_on(iface.lock());

        let socket = iface_guard
            .iface_and_sockets()
            .1
            .get_mut::<tcp::Socket>(self.handle().unwrap());

        socket
            .listen(IpEndpoint::from(self.local_addr().unwrap()), Some(backlog))
            .map_err(|_| EINVAL)?;

        Ok(())
    }

    async fn connect(&self, remote_addr: SocketAddr) -> KResult<()> {
        let mut bound_socket_guard = self.bound_socket.lock().await;

        if bound_socket_guard.is_none() {
            let bind_iface = get_ephemeral_iface(Some(remote_addr.ip()));
            *bound_socket_guard = Some(BoundSocket::new_bind(bind_iface.unwrap(), None)?);
        }

        drop(bound_socket_guard);

        let iface = self.iface().unwrap();
        let mut iface_guard = iface.lock().await;

        let (iface_inner, sockets) = iface_guard.iface_and_sockets();
        let socket = sockets.get_mut::<tcp::Socket>(self.handle().unwrap());

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

        struct ConnectFuture<'a> {
            socket: &'a TcpSocket,
        }

        impl<'a> Future for ConnectFuture<'a> {
            type Output = KResult<()>;

            fn poll(self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> Poll<Self::Output> {
                let this = self.get_mut();
                match this.socket.check_connect(cx.waker()) {
                    Some(result) => Poll::Ready(result),
                    None if this.socket.is_nonblock => Poll::Ready(Err(EAGAIN)),
                    None => Poll::Pending,
                }
            }
        }

        ConnectFuture { socket: self }.await
    }

    async fn accept(&self) -> KResult<(Arc<dyn Socket>, SocketAddr)> {
        struct AcceptFuture<'a> {
            socket: &'a TcpSocket,
        }

        impl<'a> Future for AcceptFuture<'a> {
            type Output = KResult<(Arc<dyn Socket>, SocketAddr)>;

            fn poll(self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> Poll<Self::Output> {
                let this = self.get_mut();
                match this.socket.try_accept(cx.waker()) {
                    Some(result) => Poll::Ready(
                        result.map(|(tcp_socket, remote_addr)| (tcp_socket as _, remote_addr)),
                    ),
                    None if this.socket.is_nonblock => Poll::Ready(Err(EAGAIN)),
                    None => Poll::Pending,
                }
            }
        }

        AcceptFuture { socket: self }.await
    }

    async fn recv(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        struct RecvFuture<'a> {
            socket: &'a TcpSocket,
            buffer: &'a mut dyn Buffer,
        }

        impl<'a> Future for RecvFuture<'a> {
            type Output = KResult<usize>;

            fn poll(self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> Poll<Self::Output> {
                let this = self.get_mut();
                match this.socket.try_recv(this.buffer, cx.waker()) {
                    Some(result) => Poll::Ready(result),
                    None if this.socket.is_nonblock => Poll::Ready(Err(EAGAIN)),
                    None => Poll::Pending,
                }
            }
        }

        RecvFuture {
            socket: self,
            buffer,
        }
        .await
    }

    async fn send(&self, stream: &mut dyn Stream) -> KResult<usize> {
        struct SendFuture<'a> {
            socket: &'a TcpSocket,
            stream: &'a mut dyn Stream,
        }

        impl<'a> Future for SendFuture<'a> {
            type Output = KResult<usize>;

            fn poll(self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> Poll<Self::Output> {
                let this = self.get_mut();
                match this.socket.try_send(this.stream, cx.waker()) {
                    Some(result) => Poll::Ready(result),
                    None if this.socket.is_nonblock => Poll::Ready(Err(EAGAIN)),
                    None => Poll::Pending,
                }
            }
        }

        SendFuture {
            socket: self,
            stream,
        }
        .await
    }
}
