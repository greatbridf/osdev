#pragma once

#include <types/stdint.h>

ssize_t
snprint_decimal(
    char* buf,
    size_t buf_size,
    int32_t num);

ssize_t
snprintf(
    char* buf,
    size_t buf_size,
    const char* fmt,
    ...);
