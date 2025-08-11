use core::net::Ipv4Addr;

use bitflags::bitflags;
use int_to_c_enum::TryFromInt;

// definition copy from asterinas

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromInt)]
#[expect(non_camel_case_types)]
pub enum SockDomain {
    AF_UNSPEC = 0,
    /// Unix domain sockets
    AF_UNIX = 1,
    // POSIX name for AF_UNIX
    // AF_LOCAL = 1,
    /// Internet IP Protocol
    AF_INET = 2,
    /// Amateur Radio AX.25
    AF_AX25 = 3,
    /// Novell IPX
    AF_IPX = 4,
    /// AppleTalk DDP
    AF_APPLETALK = 5,
    /// Amateur Radio NET/ROM
    AF_NETROM = 6,
    /// Multiprotocol bridge
    AF_BRIDGE = 7,
    /// ATM PVCs
    AF_ATMPVC = 8,
    /// Reserved for X.25 project
    AF_X25 = 9,
    /// IP version 6,
    AF_INET6 = 10,
    /// Amateur Radio X.25 PLP
    AF_ROSE = 11,
    /// Reserved for DECnet project
    AF_DECnet = 12,
    /// Reserved for 802.2LLC project
    AF_NETBEUI = 13,
    /// Security callback pseudo AF
    AF_SECURITY = 14,
    /// PF_KEY key management API
    AF_KEY = 15,
    AF_NETLINK = 16,
    // Alias to emulate 4.4BSD
    // AF_ROUTE = AF_NETLINK
    /// Packet family
    AF_PACKET = 17,
    /// Ash
    AF_ASH = 18,
    /// Acorn Econet
    AF_ECONET = 19,
    /// ATM SVCs
    AF_ATMSVC = 20,
    /// RDS sockets
    AF_RDS = 21,
    /// Linux SNA Project (nutters!)
    AF_SNA = 22,
    /// IRDA sockets
    AF_IRDA = 23,
    /// PPPoX sockets
    AF_PPPOX = 24,
    /// Wanpipe API Sockets
    AF_WANPIPE = 25,
    /// Linux LLC
    AF_LLC = 26,
    /// Native InfiniBand address
    AF_IB = 27,
    /// MPLS
    AF_MPLS = 28,
    /// Controller Area Network
    AF_CAN = 29,
    /// TIPC sockets
    AF_TIPC = 30,
    /// Bluetooth sockets
    AF_BLUETOOTH = 31,
    /// IUCV sockets
    AF_IUCV = 32,
    /// RxRPC sockets
    AF_RXRPC = 33,
    /// mISDN sockets
    AF_ISDN = 34,
    /// Phonet sockets
    AF_PHONET = 35,
    /// IEEE802154 sockets
    AF_IEEE802154 = 36,
    /// CAIF sockets
    AF_CAIF = 37,
    /// Algorithm sockets
    AF_ALG = 38,
    /// NFC sockets
    AF_NFC = 39,
    /// vSockets
    AF_VSOCK = 40,
    /// Kernel Connection Multiplexor
    AF_KCM = 41,
    /// Qualcomm IPC Router
    AF_QIPCRTR = 42,
    /// smc sockets: reserve number for
    /// PF_SMC protocol family that
    /// reuses AF_INET address family
    AF_SMC = 43,
    /// XDP sockets
    AF_XDP = 44,
    /// Management component transport protocol
    AF_MCTP = 45,
}

pub const SOCK_TYPE_MASK: u32 = 0xf;

bitflags! {
    #[repr(C)]
    pub struct SockFlags: u32 {
        const SOCK_NONBLOCK = 1 << 11;
        const SOCK_CLOEXEC = 1 << 19;
    }
}

#[repr(u32)]
#[expect(non_camel_case_types)]
#[derive(Debug, Clone, Copy, TryFromInt)]
pub enum SockType {
    /// Stream socket
    SOCK_STREAM = 1,
    /// Datagram socket
    SOCK_DGRAM = 2,
    /// Raw socket
    SOCK_RAW = 3,
    /// Reliably-delivered message
    SOCK_RDM = 4,
    /// Sequential packet socket
    SOCK_SEQPACKET = 5,
    /// Datagram Congestion Control Protocol socket
    SOCK_DCCP = 6,
    /// Linux specific way of getting packets at the dev level
    SOCK_PACKET = 10,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, TryFromInt)]
#[expect(non_camel_case_types)]
pub enum Protocol {
    IPPROTO_IP = 0,         /* Dummy protocol for TCP		*/
    IPPROTO_ICMP = 1,       /* Internet Control Message Protocol	*/
    IPPROTO_IGMP = 2,       /* Internet Group Management Protocol	*/
    IPPROTO_TCP = 6,        /* Transmission Control Protocol	*/
    IPPROTO_EGP = 8,        /* Exterior Gateway Protocol		*/
    IPPROTO_PUP = 12,       /* PUP protocol				*/
    IPPROTO_UDP = 17,       /* User Datagram Protocol		*/
    IPPROTO_IDP = 22,       /* XNS IDP protocol			*/
    IPPROTO_TP = 29,        /* SO Transport Protocol Class 4	*/
    IPPROTO_DCCP = 33,      /* Datagram Congestion Control Protocol */
    IPPROTO_IPV6 = 41,      /* IPv6-in-IPv4 tunnelling		*/
    IPPROTO_RSVP = 46,      /* RSVP Protocol			*/
    IPPROTO_GRE = 47,       /* Cisco GRE tunnels (rfc 1701,1702)	*/
    IPPROTO_ESP = 50,       /* Encapsulation Security Payload protocol */
    IPPROTO_AH = 51,        /* Authentication Header protocol	*/
    IPPROTO_MTP = 92,       /* Multicast Transport Protocol		*/
    IPPROTO_BEETPH = 94,    /* IP option pseudo header for BEET	*/
    IPPROTO_ENCAP = 98,     /* Encapsulation Header			*/
    IPPROTO_PIM = 103,      /* Protocol Independent Multicast	*/
    IPPROTO_COMP = 108,     /* Compression Header Protocol		*/
    IPPROTO_SCTP = 132,     /* Stream Control Transport Protocol	*/
    IPPROTO_UDPLITE = 136,  /* UDP-Lite (RFC 3828)			*/
    IPPROTO_MPLS = 137,     /* MPLS in IP (RFC 4023)		*/
    IPPROTO_ETHERNET = 143, /* Ethernet-within-IPv6 Encapsulation	*/
    IPPROTO_RAW = 255,      /* Raw IP packets			*/
    IPPROTO_MPTCP = 262,    /* Multipath TCP connection		*/
}

pub const ADDR_MAX_LEN: usize = 128;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CSockAddr {
    pub sa_family: u16,
    pub bytes: [u8; ADDR_MAX_LEN - 2],
    pub _align: [u64; 0],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CSocketAddrInet {
    /// Address family (AF_INET).
    sin_family: u16,
    /// Port number.
    sin_port: u16,
    /// IPv4 address.
    sin_addr: u32,
    /// Pad bytes to 16-byte `struct sockaddr`.
    sin_zero: [u8; 8],
}

impl CSocketAddrInet {
    pub fn new(addr: Ipv4Addr, port: u16) -> Self {
        Self {
            sin_family: 2, // AF_INET = 2,
            sin_port: port.to_be(),
            sin_addr: u32::from_ne_bytes(addr.octets()),
            sin_zero: [0; 8],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MsgHdr {
    /// Pointer to socket address structure
    pub msg_name: usize,
    /// Size of socket address
    pub msg_namelen: i32,
    /// Scatter/Gather iov array
    pub msg_iov: usize,
    /// The # of elements in msg_iov
    pub msg_iovlen: u32,
    /// Ancillary data
    pub msg_control: usize,
    /// Ancillary data buffer length
    pub msg_controllen: u32,
    /// Flags on received message
    pub msg_flags: u32,
}
