#include <stdint.h>

#define BYTES_PER_MAX_COPY_UNIT (sizeof(uint32_t) / sizeof(uint8_t))

void* memcpy(void* _dst, const void* _src, size_t n)
{
    void* orig_dst = _dst;
    uint8_t* dst = (uint8_t*)_dst;
    const uint8_t* src = (const uint8_t*)_src;
    for (size_t i = 0; i < n / BYTES_PER_MAX_COPY_UNIT; ++i) {
        *(uint32_t*)dst = *(uint32_t*)src;
        dst += BYTES_PER_MAX_COPY_UNIT;
        src += BYTES_PER_MAX_COPY_UNIT;
    }
    for (size_t i = 0; i < (n % BYTES_PER_MAX_COPY_UNIT); ++i) {
        *((char*)dst++) = *((char*)src++);
    }
    return orig_dst;
}

void* memset(void* _dst, int c, size_t n)
{
    uint8_t* dst = (uint8_t*)_dst;
    c &= 0xff;
    int cc = (c + (c << 8) + (c << 16) + (c << 24));
    for (size_t i = 0; i < n / BYTES_PER_MAX_COPY_UNIT; ++i) {
        *(uint32_t*)dst = cc;
        dst += BYTES_PER_MAX_COPY_UNIT;
    }
    for (size_t i = 0; i < (n % BYTES_PER_MAX_COPY_UNIT); ++i) {
        *((char*)dst++) = c;
    }
    return dst;
}

size_t strlen(const char* str)
{
    size_t n = 0;
    while (*(str++) != '\0')
        ++n;
    return n;
}

char* strncpy(char* dst, const char* src, size_t n)
{
    size_t len = strlen(src);

    if (len < n) {
        memset(dst + len, 0x00, n - len);
        memcpy(dst, src, len);
    } else {
        memcpy(dst, src, n);
    }

    return dst;
}

int strcmp(const char* s1, const char* s2)
{
    int c;
    while ((c = *s1 - *s2) == 0 && *s1 != 0) {
        ++s1;
        ++s2;
    }
    return c;
}
