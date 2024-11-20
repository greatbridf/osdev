#pragma once

#include <bits/alltypes.h>
#include <stdint.h>
#include <sys/stat.h>
#include <sys/types.h>

#define NODE_MAJOR(node) (((node) >> 8) & 0xFFU)
#define NODE_MINOR(node) ((node) & 0xFFU)

namespace fs {

constexpr dev_t make_device(uint32_t major, uint32_t minor) {
    return ((major << 8) & 0xFF00U) | (minor & 0xFFU);
}

} // namespace fs
