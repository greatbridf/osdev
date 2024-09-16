#pragma once

#include <types/types.h>

using u8 = uint8_t;
using u16 = uint16_t;
using u32 = uint32_t;
using u64 = uint64_t;
using usize = size_t;

using i8 = char;
using i16 = short;
using i32 = int;
using i64 = long long;
using isize = long;

template <typename T>
constexpr bool test(T x, T y) {
    return (x & y) == y;
}
