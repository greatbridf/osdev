#include <net/ethernet.hpp>

u16 net::htons(u16 x) {
    return __builtin_bswap16(x);
}

u32 net::htonl(u32 x) {
    return __builtin_bswap32(x);
}

u64 net::htonll(u64 x) {
    return __builtin_bswap64(x);
}
