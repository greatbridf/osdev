#pragma once

#include <types/stdint.h>

#ifndef CR
#define CR ('\r')
#endif

#ifndef LF
#define LF ('\n')
#endif

#ifdef __cplusplus
extern "C" {
#endif

void* memcpy(void* dst, const void* src, size_t n);
void* memset(void* dst, int c, size_t n);
size_t strlen(const char* str);
char* strncpy(char* dst, const char* src, size_t max_n);

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

#ifdef __cplusplus
}
#endif
