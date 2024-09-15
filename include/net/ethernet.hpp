#pragma once

#include <defs.hpp>

namespace net {

u16 htons(u16 val);
u32 htonl(u32 val);
u64 htonll(u64 val);

struct PACKED MACAddress {
    u8 addr[6];
};

struct PACKED EthernetHeader {
    MACAddress dst;
    MACAddress src;
    u16 type;
    u8 payload[];
};

} // namespace net
