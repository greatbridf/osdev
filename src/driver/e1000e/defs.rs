// Register names
// Control register
pub const REG_CTRL: u32 = 0x00000;
// Status register
pub const REG_STAT: u32 = 0x00008;
// Interrupt cause register
pub const REG_ICR: u32 = 0x000C0;
// Interrupt mask set/clear register
pub const REG_IMS: u32 = 0x000D0;
// Interrupt mask clear register
pub const REG_IMC: u32 = 0x000D8;
// Multicast table array start
pub const REG_MTA: u32 = 0x05200;

// Receive control
pub const REG_RCTL: u32 = 0x00100;

// These registers are per-queue

// Receive descriptor base address low
pub const REG_RDBAL: u32 = 0x02800;
// Receive descriptor base address high
pub const REG_RDBAH: u32 = 0x02804;
// Receive descriptor length
pub const REG_RDLEN: u32 = 0x02808;
// Receive descriptor head
pub const REG_RDH: u32 = 0x02810;
// Receive descriptor tail
pub const REG_RDT: u32 = 0x02818;
// Receive descriptor control
pub const REG_RXDCTL: u32 = 0x02828;

// Transmit control
pub const REG_TCTL: u32 = 0x00400;

// These registers are per-queue

// Transmit descriptor base address low
pub const REG_TDBAL: u32 = 0x03800;
// Transmit descriptor base address high
pub const REG_TDBAH: u32 = 0x03804;
// Transmit descriptor length
pub const REG_TDLEN: u32 = 0x03808;
// Transmit descriptor head
pub const REG_TDH: u32 = 0x03810;
// Transmit descriptor tail
pub const REG_TDT: u32 = 0x03818;
// Transmit descriptor control
pub const REG_TXDCTL: u32 = 0x03828;

// Collision counter
pub const REG_COLC: u32 = 0x04028;
// Good packets received counter
pub const REG_GPRC: u32 = 0x04074;
// Multicast packets received counter
pub const REG_MPRC: u32 = 0x0407C;
// Good packets transmitted counter
pub const REG_GPTC: u32 = 0x04080;
// Good octets received counter low
pub const REG_GORCL: u32 = 0x04088;
// Good octets received counter high
pub const REG_GORCH: u32 = 0x0408C;
// Good octets transmitted counter low
pub const REG_GOTCL: u32 = 0x04090;
// Good octets transmitted counter high
pub const REG_GOTCH: u32 = 0x04094;

// Full-duplex
pub const CTRL_FD: u32 = 0x00000001;
// GIO master disable
pub const CTRL_GIOD: u32 = 0x00000004;
// Set link up
pub const CTRL_SLU: u32 = 0x00000040;
// Software reset
pub const CTRL_RST: u32 = 0x04000000;
// Receive flow control enable
pub const CTRL_RFCE: u32 = 0x08000000;
// Transmit flow control enable
pub const CTRL_TFCE: u32 = 0x10000000;

// Full-duplex
pub const STAT_FD: u32 = 0x00000001;
// Link up
pub const STAT_LU: u32 = 0x00000002;
// Transmit paused
pub const STAT_TXOFF: u32 = 0x00000010;
// Link speed settings
pub const STAT_SPEED_MASK: u32 = 0x000000C0;
pub const STAT_SPEED_10M: u32 = 0x00000000;
pub const STAT_SPEED_100M: u32 = 0x00000040;
pub const STAT_SPEED_1000M: u32 = 0x00000080;
// GIO master enable
pub const STAT_GIOE: u32 = 0x00080000;

// Receive control enable
pub const RCTL_EN: u32 = 0x00000002;
// Store bad packets
pub const RCTL_SBP: u32 = 0x00000004;
// Unicast promiscuous mode
pub const RCTL_UPE: u32 = 0x00000008;
// Multicast promiscuous mode
pub const RCTL_MPE: u32 = 0x00000010;
// Long packet enable
pub const RCTL_LPE: u32 = 0x00000020;
// Loopback mode
pub const RCTL_LBM_MASK: u32 = 0x000000C0;
pub const RCTL_LBM_NO: u32 = 0x00000000;
pub const RCTL_LBM_MAC: u32 = 0x00000040;
// Receive descriptor minimum threshold size
pub const RCTL_RDMTS_MASK: u32 = 0x00000300;
pub const RCTL_RDMTS_HALF: u32 = 0x00000000;
pub const RCTL_RDMTS_QUARTER: u32 = 0x00000100;
pub const RCTL_RDMTS_EIGHTH: u32 = 0x00000200;
// Receive descriptor type
pub const RCTL_DTYP_MASK: u32 = 0x00000C00;
pub const RCTL_DTYP_LEGACY: u32 = 0x00000000;
pub const RCTL_DTYP_SPLIT: u32 = 0x00000400;
// Multicast offset
pub const RCTL_MO_MASK: u32 = 0x00003000;
// Broadcast accept mode
pub const RCTL_BAM: u32 = 0x00008000;
// Receive buffer size
pub const RCTL_BSIZE_MASK: u32 = 0x00030000;
pub const RCTL_BSIZE_2048: u32 = 0x00000000;
pub const RCTL_BSIZE_1024: u32 = 0x00010000;
pub const RCTL_BSIZE_512: u32 = 0x00020000;
pub const RCTL_BSIZE_256: u32 = 0x00030000;
// VLAN filter enable
pub const RCTL_VFE: u32 = 0x00040000;
// Canonical form indicator enable
pub const RCTL_CFIEN: u32 = 0x00080000;
// Canonical form indicator bit value
pub const RCTL_CFI: u32 = 0x00100000;
// Discard pause frames
pub const RCTL_DPF: u32 = 0x00400000;
// Pass MAC control frames
pub const RCTL_PMCF: u32 = 0x00800000;
// Buffer size extension
pub const RCTL_BSEX: u32 = 0x02000000;
// Strip Ethernet CRC
pub const RCTL_SECRC: u32 = 0x04000000;
// Flexible buffer size
pub const RCTL_FLXBUF_MASK: u32 = 0x78000000;
pub const RCTL_BSIZE_16384: u32 = RCTL_BSIZE_1024 | RCTL_BSEX;
pub const RCTL_BSIZE_8192: u32 = RCTL_BSIZE_512 | RCTL_BSEX;
pub const RCTL_BSIZE_4096: u32 = RCTL_BSIZE_256 | RCTL_BSEX;

// Transmit control enable
pub const TCTL_EN: u32 = 0x00000002;
// Pad short packets
pub const TCTL_PSP: u32 = 0x00000008;
// Collision threshold
pub const TCTL_CT_MASK: u32 = 0x00000ff0;
pub const TCTL_CT_SHIFT: u32 = 4;
// Collision distance
pub const TCTL_COLD_MASK: u32 = 0x003ff000;
pub const TCTL_COLD_SHIFT: u32 = 12;
// Software XOFF transmission
pub const TCTL_SWXOFF: u32 = 0x00400000;
// Packet burst enable
pub const TCTL_PBE: u32 = 0x00800000;
// Re-transmit on late collision
pub const TCTL_RTLC: u32 = 0x01000000;

// Transmit descriptor written back
pub const ICR_TXDW: u32 = 0x00000001;
// Link status change
pub const ICR_LSC: u32 = 0x00000004;
// Receive sequence error
pub const ICR_RXSEQ: u32 = 0x00000008;
// Receive descriptor minimum threshold
pub const ICR_RXDMT0: u32 = 0x00000010;
// Receive overrun
pub const ICR_RXO: u32 = 0x00000040;
// Receive timer expired
pub const ICR_RXT0: u32 = 0x00000080;
// MDIO access complete
pub const ICR_MDAC: u32 = 0x00000200;
// Transmit descriptor low minimum threshold
pub const ICR_TXD_LOW: u32 = 0x00008000;
// Small packet received
pub const ICR_SRPD: u32 = 0x00010000;
// ACK frame received
pub const ICR_ACK: u32 = 0x0020000;
// Management frame received
pub const ICR_MNG: u32 = 0x0040000;
// Other interrupt
pub const ICR_OTHER: u32 = 0x01000000;
// Interrupt asserted
pub const ICR_INT: u32 = 0x80000000;

pub const ICR_NORMAL: u32 =
    ICR_LSC | ICR_RXO | ICR_MDAC | ICR_SRPD | ICR_ACK | ICR_MNG;
pub const ICR_UP: u32 = ICR_TXDW | ICR_RXSEQ | ICR_RXDMT0 | ICR_RXT0;

pub const RXD_STAT_DD: u8 = 0x01;

pub const TXD_STAT_DD: u8 = 0x01;

pub const TXD_CMD_EOP: u8 = 0x01;
pub const TXD_CMD_IFCS: u8 = 0x02;
pub const TXD_CMD_RS: u8 = 0x08;
