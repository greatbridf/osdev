#pragma once

#include <stdint.h>

#include <types/types.h>

namespace types {

using ptr32_t = uint32_t;

struct iovec32 {
    ptr32_t iov_base;
    ptr32_t iov_len;
};

} // namespace types
