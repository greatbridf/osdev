use super::netdev::AnyNetDevice;
use alloc::{collections::BTreeSet, sync::Arc};
use core::marker::PhantomData;
use smoltcp::{iface::SocketHandle, socket::AnySocket, wire::IpAddress};

pub struct BoundSocket {
    address: IpAddress,
    port: u16,

    handle: SocketHandle,
    net_device: Arc<dyn AnyNetDevice>,
}

pub enum Socket {
    Unbound,
    Bound(Arc<BoundSocket>),
}

pub struct SocketSet<T>
where
    T: for<'a> AnySocket<'a>,
{
    sockets: BTreeSet<Arc<BoundSocket>>,
    _phantom: PhantomData<T>,
}
