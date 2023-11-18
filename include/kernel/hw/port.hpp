#pragma once

#include <stdint.h>
#include <type_traits>

namespace hw {
template <typename port_size_t, bool r = true, bool w = true>
class port {
private:
    uint16_t mp;

public:
    explicit port(uint16_t p)
        : mp(p)
    {
    }

    port_size_t operator*(void) const
    {
        static_assert(
            std::is_same_v<port_size_t, uint8_t> || std::is_same_v<port_size_t, uint16_t>,
            "this type is not implemented yet.");
        port_size_t ret;
        if constexpr (std::is_same_v<port_size_t, uint8_t>)
            asm volatile(
                "inb %1, %0"
                : "=a"(ret)
                : "d"(mp));
        if constexpr (std::is_same_v<port_size_t, uint16_t>)
            asm volatile(
                "inw %1, %0"
                : "=a"(ret)
                : "d"(mp));
        return ret;
    }

    port_size_t operator=(port_size_t n) const
    {
        static_assert(
            std::is_same_v<port_size_t, uint8_t> || std::is_same_v<port_size_t, uint16_t>,
            "this type is not implemented yet.");
        if constexpr (std::is_same_v<port_size_t, uint8_t>)
            asm volatile(
                "outb %1, %0"
                :
                : "d"(mp), "a"(n));
        if constexpr (std::is_same_v<port_size_t, uint16_t>)
            asm volatile(
                "outw %1, %0"
                :
                : "d"(mp), "a"(n));
        return n;
    }
};

using p8 = port<uint8_t>;
using p8r = port<uint8_t, true, false>;
using p8w = port<uint8_t, false, true>;
using p16 = port<uint16_t>;
using p16r = port<uint16_t, true, false>;
using p16w = port<uint16_t, false, true>;

template <>
uint8_t p8r::operator=(uint8_t n) const = delete;
template <>
uint8_t p8w::operator*(void) const = delete;
template <>
uint16_t p16r::operator=(uint16_t n) const = delete;
template <>
uint16_t p16w::operator*(void) const = delete;
} // namespace hw
