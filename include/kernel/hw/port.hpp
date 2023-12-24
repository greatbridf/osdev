#pragma once

#include <stdint.h>

namespace kernel::hw {

inline uint32_t inl(uint16_t pn)
{
    uint32_t ret;
    asm volatile("inl %1, %0"
        : "=a"(ret)
        : "d"(pn));
    return ret;
}

inline uint32_t outl(uint16_t pn, uint32_t n)
{
    asm volatile("outl %1, %0"
        :
        : "d"(pn), "a"(n));
    return n;
}

inline uint16_t inw(uint16_t pn)
{
    uint16_t ret;
    asm volatile("inw %1, %0"
        : "=a"(ret)
        : "d"(pn));
    return ret;
}

inline uint16_t outw(uint16_t pn, uint16_t n)
{
    asm volatile("outw %1, %0"
        :
        : "d"(pn), "a"(n));
    return n;
}

inline uint8_t inb(uint16_t pn)
{
    uint8_t ret;
    asm volatile("inb %1, %0"
        : "=a"(ret)
        : "d"(pn));
    return ret;
}

inline uint8_t outb(uint16_t pn, uint8_t n)
{
    asm volatile("outb %1, %0"
        :
        : "d"(pn), "a"(n));
    return n;
}

struct p32 {
    uint16_t mp;

    explicit constexpr p32(uint16_t p) : mp(p) { }
    inline uint32_t operator*() const { return inl(mp); }
    inline uint32_t operator=(uint32_t n) const { return outl(mp, n); }
};

struct p16 {
    uint16_t mp;

    explicit constexpr p16(uint16_t p) : mp(p) { }
    inline uint16_t operator*() const { return inw(mp); }
    inline uint16_t operator=(uint16_t n) const { return outw(mp, n); }
};

struct p8 {
    uint16_t mp;

    explicit constexpr p8(uint16_t p) : mp(p) { }
    inline uint8_t operator*() const { return inb(mp); }
    inline uint8_t operator=(uint8_t n) const { return outb(mp, n); }
};

} // namespace kernel::hw

namespace hw {

// for backward compatibility
using p8 = kernel::hw::p8;
using p8r = kernel::hw::p8;
using p8w = kernel::hw::p8;
using p16 = kernel::hw::p16;
using p16r = kernel::hw::p16;
using p16w = kernel::hw::p16;

} // namespace hw
