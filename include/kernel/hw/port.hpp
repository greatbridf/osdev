#pragma once

#include <stdint.h>
#include <types/cplusplus.hpp>

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
            types::is_same<port_size_t, uint8_t>::value || types::is_same<port_size_t, uint16_t>::value,
            "this type is not implemented yet.");
        port_size_t ret;
        if constexpr (types::is_same<port_size_t, uint8_t>::value)
            asm volatile(
                "inb %1, %0"
                : "=a"(ret)
                : "d"(mp));
        if constexpr (types::is_same<port_size_t, uint16_t>::value)
            asm volatile(
                "inw %1, %0"
                : "=a"(ret)
                : "d"(mp));
        return ret;
    }

    port_size_t operator=(port_size_t n) const
    {
        static_assert(
            types::is_same<port_size_t, uint8_t>::value || types::is_same<port_size_t, uint16_t>::value,
            "this type is not implemented yet.");
        if constexpr (types::is_same<port_size_t, uint8_t>::value)
            asm volatile(
                "outb %1, %0"
                :
                : "d"(mp), "a"(n));
        if constexpr (types::is_same<port_size_t, uint16_t>::value)
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
