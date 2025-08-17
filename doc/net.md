# 网络栈

Eonix的网络协议栈基于 smoltcp 设计。由于 smoltcp 是一个专为嵌入式环境设计的 Rust 网络栈，相关网络接口与 posix 规范并不对齐，我们通过对 smoltcp 进行面向 posix 网络规范的改造与封装，实现了从底层网络设备驱动到高层应用程序套接字的网络协议栈支持。Eonix 贯彻 smoltcp 设计理念，将网络功能划分为设备层，接口层，套接字层三个核心层次，优雅实现了网络协议栈的层次抽象。同时 Eonix 网络层利用 Rust 高效异步特性，有效提升网络性能和系统灵活性。

## 核心架构

整个网络模块分为三个核心层次：

- 设备层 (Device Layer)：负责与具体的物理或虚拟网络设备进行交互，如 VirtIO 网卡或回环设备。它实现了 smoltcp::phy::Device Trait，为上层提供统一的数据包收发接口。
- 接口层 (Interface Layer)：管理网络接口卡（NIC）的配置，包括 IP 地址、MAC 地址等。它维护了一个 smoltcp::iface::Interface 实例和一个套接字集合 (smoltcp::iface::SocketSet)。
- 套接字层 (Socket Layer)：提供用户态应用程序使用的抽象，如 TcpSocket 和 UdpSocket。它封装了 smoltcp 的底层套接字，并提供了 bind, listen, connect, send, recv 等标准 API，并支持异步操作。

## 设备层

设备层是整个网络栈的基础，它将物理硬件抽象为统一的 NetDevice 对象。NetDev Trait 定义了所有网络设备必须实现的接口，如 name(), mac_addr(), recv(), send() 等。这使得上层代码可以与具体的设备类型解耦。

``` rust
pub type NetDevice = Arc<Mutex<dyn NetDev>>;

pub trait NetDev: Send {
    fn name(&self) -> &'static str;
    fn mac_addr(&self) -> Mac;
    fn caps(&self) -> DeviceCapabilities;
    fn can_receive(&self) -> bool;
    fn can_send(&self) -> bool;
    fn recv(&mut self) -> Result<RxBuffer, NetDevError>;
    fn recycle_rx_buffer(&mut self, rx_buffer: RxBuffer) -> Result<(), NetDevError>;
    fn send(&mut self, data: &[u8]) -> Result<(), NetDevError>;
}
```

smoltcp::phy::Device 实现: NetDev Trait 通过 impl smoltcp::phy::Device for dyn NetDev 来适配 smoltcp 的物理设备接口。它将 recv 和 send 方法映射到 smoltcp 的 receive 和 transmit 方法上，实现了协议栈与设备的交互。

``` rust
// 实现 smoltcp 的 Device Trait，将 NetDev 适配到协议栈
impl smoltcp::phy::Device for dyn NetDev {
    type RxToken<'a> = RxToken where Self: 'a;
    type TxToken<'a> = TxToken<'a> where Self: 'a;

    fn receive(&mut self, _timestamp: smoltcp::time::Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        if self.can_receive() && self.can_send() {
            let rx_buffer = self.recv().unwrap();
            Some((RxToken(rx_buffer), TxToken(self)))
        } else {
            None
        }
    }
    // ... (transmit, capabilities 的实现)
}
```

## 接口层

接口层 (Iface) 负责管理一个网络接口的协议栈状态。

- Iface 结构体: 封装了 NetDevice、smoltcp::iface::Interface 和 smoltcp::iface::SocketSet。它将设备层和套接字层连接起来。

- 端口管理: 使用 BTreeSet<(SocketType, u16)> 来追踪已使用的端口，并提供了 alloc_port 和 bind_socket 方法来分配和绑定端口，防止端口冲突。

- 定时轮询: Iface::poll() 方法会定期调用 smoltcp::iface::Interface::poll() 来处理传入和传出的数据包，并更新套接字状态。这是整个异步 I/O 机制的核心驱动力。

``` rust
pub struct Iface {
    device: NetDevice,
    iface_inner: Interface,
    used_ports: BTreeSet<(SocketType, u16)>,
    sockets: SocketSet<'static>,
}

impl Iface {
    pub fn poll(&mut self) {
        let mut device = Task::block_on(self.device.lock());
        let timestamp = smoltcp::time::Instant::from_millis(Instant::now().to_millis() as i64);
        self.iface_inner.poll(timestamp, &mut *device, &mut self.sockets);
    }
}
```

## 套接字层

套接字层提供了与 POSIX 类似但基于 Rust async/await 的 API。

- Socket Trait: 定义了通用的套接字接口，所有具体协议的套接字都必须实现此 Trait。

``` rust
# [async_trait]
pub trait Socket: Sync + Send {
    fn local_addr(&self) -> Option<SocketAddr>;
    fn remote_addr(&self) -> Option<SocketAddr>;
    fn bind(&self, socket_addr: SocketAddr) -> KResult<()>;
    fn listen(&self, backlog: usize) -> KResult<()>;
    async fn connect(&self, remote_addr: SocketAddr) -> KResult<()>;
    async fn accept(&self) -> KResult<Arc<dyn Socket>>;
    async fn recv(&self, buffer: &mut dyn Buffer) -> KResult<(usize, RecvMetadata)>;
    async fn send(&self, stream: &mut dyn Stream, send_meta: SendMetadata) -> KResult<usize>;
    async fn poll(&self, events: PollEvent) -> KResult<PollEvent>;
}
```

- TcpSocket/UdpSocket 结构体: 封装了 smoltcp socket ,并在结构体中存储热点数据，实现更加细粒度的并发管理。

``` rust
pub struct TcpSocket {
    bound_socket: RwLock<Option<BoundSocket>>,
    local_addr: RwLock<Option<SocketAddr>>,
    remote_addr: RwLock<Option<SocketAddr>>,
    is_nonblock: bool,
}
```

- 异步 I/O: recv() 和 send() 等方法被实现为 async fn，内部通过 Future 状态机来处理非阻塞操作。当 smoltcp 的套接字缓冲区不可用时，它会注册 waker，并在 poll 方法中唤醒任务。

``` rust
impl TcpSocket {
    n try_recv(
        &self,
        buf: &mut dyn Buffer,
        waker: &Waker,
    ) -> KResult<Option<(usize, RecvMetadata)>> {
        // ... socket 并发访问控制
        if !socket.may_recv() {
            Err(ENOTCONN)
        } else if socket.can_recv() {
            let len = socket
                .recv(|rx_data| {
                    let _ = buf.fill(&rx_data[..]);
                    (buf.wrote(), buf.wrote())
                })
                .unwrap();
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
}

impl Socket for TcpSocket {
/// ... bind、send等实现
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
}
```

- 资源管理: TcpSocket::drop 方法确保在套接字被销毁时，正确地从接口的套接字集合中移除并释放端口资源，避免资源泄漏。
