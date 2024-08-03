#pragma once
#include <bit>
#include <utility>

#include <stdint.h>

#include <types/path.hpp>
#include <types/types.h>

namespace types {

// taken from linux
constexpr uint64_t GOLDEN_RATIO_64 = 0x61C8864680B583EBull;

using hash_t = std::size_t;

constexpr hash_t hash(uint64_t val, int bits)
{
    // higher bits are more random
    return (val * GOLDEN_RATIO_64) >> (64 - bits);
}

inline hash_t hash_ptr(void* p, int bits)
{
    return hash(std::bit_cast<uintptr_t>(p), bits);
}

inline hash_t hash_str(const char* str, int bits)
{
    constexpr hash_t seed = 131;
    hash_t tmp = 0;

    while (*str)
        tmp = tmp * seed + (*str++);

    return hash(tmp, bits);
};

inline hash_t hash_str(string_view str, int bits)
{
    constexpr hash_t seed = 131;
    hash_t tmp = 0;

    for (auto c : str)
        tmp = tmp * seed + c;

    return hash(tmp, bits);
};

} // namespace types
