#include <defs.hpp>

namespace hw::e1000e {

// Register names
// Control register
constexpr u32 REG_CTRL = 0x00000;
// Status register
constexpr u32 REG_STATUS = 0x00008;
// Interrupt cause register
constexpr u32 REG_ICR = 0x000C0;
// Interrupt mask set/clear register
constexpr u32 REG_IMS = 0x000D0;
// Interrupt mask clear register
constexpr u32 REG_IMC = 0x000D8;

// Receive control
constexpr u32 REG_RCTL = 0x00100;

// These registers are per-queue

// Receive descriptor base address low
constexpr u32 REG_RDBAL = 0x02800;
// Receive descriptor base address high
constexpr u32 REG_RDBAH = 0x02804;
// Receive descriptor length
constexpr u32 REG_RDLEN = 0x02808;
// Receive descriptor head
constexpr u32 REG_RDH = 0x02810;
// Receive descriptor tail
constexpr u32 REG_RDT = 0x02818;
// Receive descriptor control
constexpr u32 REG_RXDCTL = 0x02828;

// Transmit control
constexpr u32 REG_TCTL = 0x00400;

// These registers are per-queue

// Transmit descriptor base address low
constexpr u32 REG_TDBAL = 0x03800;
// Transmit descriptor base address high
constexpr u32 REG_TDBAH = 0x03804;
// Transmit descriptor length
constexpr u32 REG_TDLEN = 0x03808;
// Transmit descriptor head
constexpr u32 REG_TDH = 0x03810;
// Transmit descriptor tail
constexpr u32 REG_TDT = 0x03818;
// Transmit descriptor control
constexpr u32 REG_TXDCTL = 0x03828;

// Collision counter
constexpr u32 REG_COLC = 0x04028;
// Good packets received counter
constexpr u32 REG_GPRC = 0x04074;
// Multicast packets received counter
constexpr u32 REG_MPRC = 0x0407C;
// Good packets transmitted counter
constexpr u32 REG_GPTC = 0x04080;
// Good octets received counter low
constexpr u32 REG_GORCL = 0x04088;
// Good octets received counter high
constexpr u32 REG_GORCH = 0x0408C;
// Good octets transmitted counter low
constexpr u32 REG_GOTCL = 0x04090;
// Good octets transmitted counter high
constexpr u32 REG_GOTCH = 0x04094;

// Full-duplex
constexpr u32 CTRL_FD = 0x00000001;
// GIO master disable
constexpr u32 CTRL_GIOD = 0x00000004;
// Set link up
constexpr u32 CTRL_SLU = 0x00000040;
// Software reset
constexpr u32 CTRL_RST = 0x04000000;
// Receive flow control enable
constexpr u32 CTRL_RFCE = 0x08000000;
// Transmit flow control enable
constexpr u32 CTRL_TFCE = 0x10000000;

// Full-duplex
constexpr u32 STATUS_FD = 0x00000001;
// Link up
constexpr u32 STATUS_LU = 0x00000002;
// Transmit paused
constexpr u32 STATUS_TXOFF = 0x00000010;
// Link speed settings
constexpr u32 STATUS_SPEED_MASK = 0x000000C0;
constexpr u32 STATUS_SPEED_10M = 0x00000000;
constexpr u32 STATUS_SPEED_100M = 0x00000040;
constexpr u32 STATUS_SPEED_1000M = 0x00000080;
// GIO master enable
constexpr u32 STATUS_GIOE = 0x00080000;

// Receive control enable
constexpr u32 RCTL_EN = 0x00000002;
// Store bad packets
constexpr u32 RCTL_SBP = 0x00000004;
// Unicast promiscuous mode
constexpr u32 RCTL_UPE = 0x00000008;
// Multicast promiscuous mode
constexpr u32 RCTL_MPE = 0x00000010;
// Long packet enable
constexpr u32 RCTL_LPE = 0x00000020;
// Loopback mode
constexpr u32 RCTL_LBM_MASK = 0x000000C0;
constexpr u32 RCTL_LBM_NO = 0x00000000;
constexpr u32 RCTL_LBM_MAC = 0x00000040;
// Receive descriptor minimum threshold size
constexpr u32 RCTL_RDMTS_MASK = 0x00000300;
constexpr u32 RCTL_RDMTS_HALF = 0x00000000;
constexpr u32 RCTL_RDMTS_QUARTER = 0x00000100;
constexpr u32 RCTL_RDMTS_EIGHTH = 0x00000200;
// Receive descriptor type
constexpr u32 RCTL_DTYP_MASK = 0x00000C00;
constexpr u32 RCTL_DTYP_LEGACY = 0x00000000;
constexpr u32 RCTL_DTYP_SPLIT = 0x00000400;
// Multicast offset
constexpr u32 RCTL_MO_MASK = 0x00003000;
// Broadcast accept mode
constexpr u32 RCTL_BAM = 0x00008000;
// Receive buffer size
constexpr u32 RCTL_BSIZE_MASK = 0x00030000;
constexpr u32 RCTL_BSIZE_2048 = 0x00000000;
constexpr u32 RCTL_BSIZE_1024 = 0x00010000;
constexpr u32 RCTL_BSIZE_512 = 0x00020000;
constexpr u32 RCTL_BSIZE_256 = 0x00030000;
// VLAN filter enable
constexpr u32 RCTL_VFE = 0x00040000;
// Canonical form indicator enable
constexpr u32 RCTL_CFIEN = 0x00080000;
// Canonical form indicator bit value
constexpr u32 RCTL_CFI = 0x00100000;
// Discard pause frames
constexpr u32 RCTL_DPF = 0x00400000;
// Pass MAC control frames
constexpr u32 RCTL_PMCF = 0x00800000;
// Buffer size extension
constexpr u32 RCTL_BSEX = 0x02000000;
// Strip Ethernet CRC
constexpr u32 RCTL_SECRC = 0x04000000;
// Flexible buffer size
constexpr u32 RCTL_FLXBUF_MASK = 0x78000000;
constexpr u32 RCTL_BSIZE_16384 = (RCTL_BSIZE_1024 | RCTL_BSEX);
constexpr u32 RCTL_BSIZE_8192 = (RCTL_BSIZE_512 | RCTL_BSEX);
constexpr u32 RCTL_BSIZE_4096 = (RCTL_BSIZE_256 | RCTL_BSEX);

// Transmit control enable
constexpr u32 TCTL_EN = 0x00000002;
// Pad short packets
constexpr u32 TCTL_PSP = 0x00000008;
// Collision threshold
constexpr u32 TCTL_CT_MASK = 0x00000ff0;
constexpr u32 TCTL_CT_SHIFT = 4;
// Collision distance
constexpr u32 TCTL_COLD_MASK = 0x003ff000;
constexpr u32 TCTL_COLD_SHIFT = 12;
// Software XOFF transmission
constexpr u32 TCTL_SWXOFF = 0x00400000;
// Packet burst enable
constexpr u32 TCTL_PBE = 0x00800000;
// Re-transmit on late collision
constexpr u32 TCTL_RTLC = 0x01000000;

// Transmit descriptor written back
constexpr u32 ICR_TXDW = 0x00000001;
// Link status change
constexpr u32 ICR_LSC = 0x00000004;
// Receive sequence error
constexpr u32 ICR_RXSEQ = 0x00000008;
// Receive descriptor minimum threshold
constexpr u32 ICR_RXDMT0 = 0x00000010;
// Receive overrun
constexpr u32 ICR_RXO = 0x00000040;
// Receive timer expired
constexpr u32 ICR_RXT0 = 0x00000080;
// MDIO access complete
constexpr u32 ICR_MDAC = 0x00000200;
// Transmit descriptor low minimum threshold
constexpr u32 ICR_TXD_LOW = 0x00008000;
// Small packet received
constexpr u32 ICR_SRPD = 0x00010000;
// ACK frame received
constexpr u32 ICR_ACK = 0x0020000;
// Management frame received
constexpr u32 ICR_MNG = 0x0040000;
// Other interrupt
constexpr u32 ICR_OTHER = 0x01000000;
// Interrupt asserted
constexpr u32 ICR_INT = 0x80000000;

constexpr u32 ICR_NORMAL =
    (ICR_LSC | ICR_RXO | ICR_MDAC | ICR_SRPD | ICR_ACK | ICR_MNG);
constexpr u32 ICR_UP = (ICR_TXDW | ICR_RXSEQ | ICR_RXDMT0 | ICR_RXT0);

struct RxDescriptor {
    u64 bufferAddress;
    u16 length;
    u16 volatile csum; // Checksum
    u8 volatile status;
    u8 volatile errors;
    u16 vlan;
};

// Descriptor done
constexpr u8 RXD_STAT_DD = 0x01;

struct TxDescriptor {
    u64 bufferAddress;
    u16 length;
    u8 cso; // Checksum offset
    u8 cmd;
    u8 volatile status;
    u8 css; // Checksum start
    u16 vlan;
};

// Descriptor done
constexpr u8 TXD_STAT_DD = 0x01;
// End of packet
constexpr u8 TXD_CMD_EOP = 0x01;
// Insert FCS
constexpr u8 TXD_CMD_IFCS = 0x02;
// Report status
constexpr u8 TXD_CMD_RS = 0x08;

} // namespace hw::e1000e
