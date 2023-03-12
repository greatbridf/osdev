#include <kernel/stdio.hpp>
#include <kernel/tty.hpp>

#include <types/size.h>
#include <types/stdint.h>

#define __32bit_system

#ifdef __32bit_system
uint64_t do_div(uint64_t a, uint64_t b, uint64_t* remainder)
{
    uint64_t r = 0, q = 0;
    int32_t i;
    for (i = 0; i < 64; i++) {
        r = (r << 1) + (a >> 63);
        a <<= 1;
        q <<= 1;
        if (r >= b) {
            r -= b;
            q += 1;
        }
    }
    if (remainder)
        *remainder = r;
    return q;
}

int64_t do_div_s(int64_t a, int64_t b, uint64_t* remainder)
{
    int32_t qf = 0, rf = 0;
    if (a < 0) {
        qf = rf = 1;
        a = -a;
    }
    if (b < 0) {
        qf ^= 1;
        b = -b;
    }

    int64_t quotient = do_div(a, b, (uint64_t*)remainder);

    if (qf)
        quotient = -quotient;
    if (remainder && rf)
        *remainder = -*remainder;

    return quotient;
}

extern "C" int64_t __divdi3(int64_t a, int64_t b)
{
    return do_div_s(a, b, (uint64_t*)0);
}

extern "C" int64_t __moddi3(int64_t a, int64_t b)
{
    uint64_t remainder = 0;
    do_div_s(a, b, &remainder);
    return remainder;
}
#endif

// where n is in the range of [0, 9]
static inline char d_to_c(int32_t n)
{
    return '0' + n;
}

// where n is between 0 and 15
// base is either 'a' of 'A',
// depending on you want capitalized
// or not
static inline char hex_to_c(int32_t n, char base)
{
    if (n < 10) {
        // n belongs to [0, 9]
        return d_to_c(n);
    } else {
        // n belongs to [10, 15]
        return base + (n - 10);
    }
}

static inline char x_to_c(int32_t n)
{
    return hex_to_c(n, 'a');
}

static inline char X_to_c(int32_t n)
{
    return hex_to_c(n, 'A');
}

// this will check if there is still free space
// in the buffer. if so, push the char into it,
// change the value of buf_size and move pointer
// forward
//
// x: char* buf
// y: size_t buf_size
// z: char c
#define do_write_if_free(x, y, z) \
    if ((y) > 1) {                \
        *((x)++) = (z);           \
        --(y);                    \
    }

ssize_t
snprint_decimal32(
    char* buf,
    size_t buf_size,
    int32_t num)
{
    ssize_t n_write = 0;

    if (num < 0) {
        do_write_if_free(buf, buf_size, '-');
        ++n_write;
        num *= (-1);
    }

    char* orig_buf = buf;

    do {
        do_write_if_free(buf, buf_size, d_to_c(num % 10));
        num /= 10;
        ++n_write;
    } while (num != 0);

    // prepend trailing '\0'
    if (buf_size > 0)
        *buf = 0x00;

    // move buf pointer to the last digit of number
    --buf;

    // reverse output
    while (orig_buf < buf) {
        char c = *buf;
        *buf = *orig_buf;
        *orig_buf = c;
        --buf;
        ++orig_buf;
    }

    return n_write;
}

ssize_t
snprint_decimal64(
    char* buf,
    size_t buf_size,
    int64_t num)
{
    ssize_t n_write = 0;

    if (num < 0) {
        do_write_if_free(buf, buf_size, '-');
        ++n_write;
        num *= (-1);
    }

    char* orig_buf = buf;

    do {
        do_write_if_free(buf, buf_size, d_to_c(num % 10));
        num /= 10;
        ++n_write;
    } while (num != 0);

    // prepend trailing '\0'
    if (buf_size > 0)
        *buf = 0x00;

    // move buf pointer to the last digit of number
    --buf;

    // reverse output
    while (orig_buf < buf) {
        char c = *buf;
        *buf = *orig_buf;
        *orig_buf = c;
        --buf;
        ++orig_buf;
    }

    return n_write;
}

ssize_t
snprint_hex32(
    char* buf,
    size_t buf_size,
    uint32_t num,
    int32_t capitalized)
{
    ssize_t n_write = 0;

    do_write_if_free(buf, buf_size, '0');
    if (capitalized) {
        do_write_if_free(buf, buf_size, 'X');
    } else {
        do_write_if_free(buf, buf_size, 'x');
    }
    n_write += 2;

    char* orig_buf = buf;

    do {
        if (capitalized) {
            do_write_if_free(buf, buf_size, X_to_c(num % 16));
        } else {
            do_write_if_free(buf, buf_size, x_to_c(num % 16));
        }
        num /= 16;
        ++n_write;
    } while (num != 0);

    // prepend trailing '\0'
    if (buf_size > 0)
        *buf = 0x00;

    // move buf pointer to the last digit of number
    --buf;

    // reverse output
    while (orig_buf < buf) {
        char c = *buf;
        *buf = *orig_buf;
        *orig_buf = c;
        --buf;
        ++orig_buf;
    }

    return n_write;
}

ssize_t
snprint_hex64(
    char* buf,
    size_t buf_size,
    uint64_t num,
    int32_t capitalized)
{
    ssize_t n_write = 0;

    do_write_if_free(buf, buf_size, '0');
    if (capitalized) {
        do_write_if_free(buf, buf_size, 'X');
    } else {
        do_write_if_free(buf, buf_size, 'x');
    }
    n_write += 2;

    char* orig_buf = buf;

    do {
        if (capitalized) {
            do_write_if_free(buf, buf_size, X_to_c(num % 16));
        } else {
            do_write_if_free(buf, buf_size, x_to_c(num % 16));
        }
        num /= 16;
        ++n_write;
    } while (num != 0);

    // prepend trailing '\0'
    if (buf_size > 0)
        *buf = 0x00;

    // move buf pointer to the last digit of number
    --buf;

    // reverse output
    while (orig_buf < buf) {
        char c = *buf;
        *buf = *orig_buf;
        *orig_buf = c;
        --buf;
        ++orig_buf;
    }

    return n_write;
}

static inline ssize_t
snprint_char(
    char* buf,
    size_t buf_size,
    char c)
{
    if (buf_size > 1)
        *buf = c;
    return sizeof(c);
}

ssize_t
snprintf(
    char* buf,
    size_t buf_size,
    const char* fmt,
    ...)
{
    ssize_t n_write = 0;
    uint8_t* arg_ptr = ((uint8_t*)&buf) + sizeof(char*) + sizeof(size_t) + sizeof(const char*);

    for (char c; (c = *fmt) != 0x00; ++fmt) {
        if (c == '%') {
            size_t n_tmp_write = 0;

            switch (*(++fmt)) {

            // int32 decimal
            case 'd':
                n_tmp_write = snprint_decimal32(buf, buf_size, *(int32_t*)arg_ptr);
                arg_ptr += sizeof(int32_t);
                break;

            case 'x':
                n_tmp_write = snprint_hex32(buf, buf_size, *(uint32_t*)arg_ptr, 0);
                arg_ptr += sizeof(uint32_t);
                break;

            case 'X':
                n_tmp_write = snprint_hex32(buf, buf_size, *(uint32_t*)arg_ptr, 1);
                arg_ptr += sizeof(uint32_t);
                break;

            // long decimal
            case 'l':
                switch (*(++fmt)) {
                // long long aka int64
                case 'l':
                    switch (*(++fmt)) {
                    case 'd':
                        n_tmp_write = snprint_decimal64(buf, buf_size, *(int64_t*)arg_ptr);
                        break;
                    case 'x':
                        n_tmp_write = snprint_hex64(buf, buf_size, *(int64_t*)arg_ptr, 0);
                        break;
                    case 'X':
                        n_tmp_write = snprint_hex64(buf, buf_size, *(int64_t*)arg_ptr, 1);
                        break;
                    }
                    arg_ptr += sizeof(int64_t);
                    break;
                // long int aka int32
                case 'd':
                    n_tmp_write = snprint_decimal32(buf, buf_size, *(int32_t*)arg_ptr);
                    arg_ptr += sizeof(int32_t);
                    break;
                case 'x':
                    n_tmp_write = snprint_hex32(buf, buf_size, *(uint32_t*)arg_ptr, 0);
                    arg_ptr += sizeof(uint32_t);
                    break;

                case 'X':
                    n_tmp_write = snprint_hex32(buf, buf_size, *(uint32_t*)arg_ptr, 1);
                    arg_ptr += sizeof(uint32_t);
                    break;
                }
                break;

            // c string
            case 's':
                n_tmp_write = snprintf(buf, buf_size, *(const char**)arg_ptr);
                arg_ptr += sizeof(const char*);
                break;

            // int8 char
            case 'c':
                n_tmp_write = snprint_char(buf, buf_size, *(char*)arg_ptr);
                arg_ptr += sizeof(char);
                break;

            // pointer
            case 'p':
#ifdef __32bit_system
                n_tmp_write = snprint_hex32(buf, buf_size, *(ptr_t*)arg_ptr, 0);
#else
                n_tmp_write = snprint_hex64(buf, buf_size, *(ptr_t*)arg_ptr, 0);
#endif
                arg_ptr += sizeof(ptr_t);
                break;

            default:
                n_tmp_write = snprint_char(buf, buf_size, *(fmt - 1));
                break;
            }

            n_write += n_tmp_write;
            if (buf_size > 1) {
                if (buf_size > n_tmp_write) {
                    buf += n_tmp_write;
                    buf_size -= n_tmp_write;
                } else {
                    // no enough space
                    // shrink buf_size to one
                    buf += (buf_size - 1);
                    buf_size = 1;
                }
            }

        } else {
            ++n_write;
            do_write_if_free(buf, buf_size, c);
        }
    }

    if (buf_size > 0)
        *buf = 0x00;

    return n_write;
}

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

void kmsg(const char* msg)
{
    console->print(msg);
}
