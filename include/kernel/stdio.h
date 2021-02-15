#pragma once

#include <types/stdint.h>

size_t
snprint_decimal(
    char* buf,
    size_t buf_size,
    int32_t num);

size_t
snprintf(
    char* buf,
    size_t buf_size,
    const char* fmt,
    ...);
