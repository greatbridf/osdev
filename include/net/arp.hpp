#pragma once

#include <defs.hpp>

#include <net/ethernet.hpp>

namespace net {

struct PACKED ARPFrame {
    u16 l2Type;
    u16 protoType;
    u8 l2AddrLen;
    u8 protoAddrLen;
    u16 op;

    MACAddress srcMAC;
    u32 srcIP;
    MACAddress dstMAC;
    u32 dstIP;
};

} // namespace net
