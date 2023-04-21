#include <devutil.h>
#include <stdint.h>
#include <stdio.h>
#include <stdarg.h>
#include <string.h>
#include <unistd.h>

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

static inline ssize_t
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

static inline ssize_t
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

static inline ssize_t
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

static inline ssize_t
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

int snprintf(char* buf, size_t bufsize, const char* fmt, ...)
{
    va_list lst;
    va_start(lst, fmt);

    int ret = vsnprintf(buf, bufsize, fmt, lst);

    va_end(lst);

    return ret;
}

int vsnprintf(char* buf, size_t buf_size, const char* fmt, va_list arg)
{
    ssize_t n_write = 0;

    for (char c; (c = *fmt) != 0x00; ++fmt) {
        if (c == '%') {
            size_t n_tmp_write = 0;

            switch (*(++fmt)) {

            // int
            case 'd':
                n_tmp_write = snprint_decimal32(buf, buf_size, va_arg(arg, int));
                break;

            case 'x':
                n_tmp_write = snprint_hex32(buf, buf_size, va_arg(arg, unsigned int), 0);
                break;

            case 'X':
                n_tmp_write = snprint_hex32(buf, buf_size, va_arg(arg, unsigned int), 1);
                break;

            // long decimal
            case 'l':
                switch (*(++fmt)) {
                // long long aka int64
                case 'l':
                    switch (*(++fmt)) {
                    case 'd':
                        n_tmp_write = snprint_decimal64(buf, buf_size, va_arg(arg, long long));
                        break;
                    case 'x':
                        n_tmp_write = snprint_hex64(buf, buf_size, va_arg(arg, unsigned long long), 0);
                        break;
                    case 'X':
                        n_tmp_write = snprint_hex64(buf, buf_size, va_arg(arg, unsigned long long), 1);
                        break;
                    }
                    break;
                // long int aka int32
                case 'd':
                    n_tmp_write = snprint_decimal32(buf, buf_size, va_arg(arg, long));
                    break;
                case 'x':
                    n_tmp_write = snprint_hex32(buf, buf_size, va_arg(arg, unsigned long), 0);
                    break;

                case 'X':
                    n_tmp_write = snprint_hex32(buf, buf_size, va_arg(arg, unsigned long), 1);
                    break;
                }
                break;

            // c string
            case 's':
                n_tmp_write = snprintf(buf, buf_size, va_arg(arg, const char*));
                break;

            // int8 char
            case 'c':
                n_tmp_write = snprint_char(buf, buf_size, va_arg(arg, int));
                break;

            // pointer
            case 'p':
#ifdef __32bit_system
                n_tmp_write = snprint_hex32(buf, buf_size, va_arg(arg, size_t), 0);
#else
                n_tmp_write = snprint_hex64(buf, buf_size, va_arg(arg, size_t), 0);
#endif
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

int sprintf(char* buf, const char* fmt, ...)
{
    va_list lst;
    va_start(lst, fmt);

    int ret = vsnprintf(buf, __SIZE_MAX__, fmt, lst);

    va_end(lst);

    return ret;
}

int puts(const char* str)
{
    int len = 0;

    int ret = write(STDOUT_FILENO, str, strlen(str));
    if (ret < 0)
        return EOF;
    len += ret;

    ret = write(STDOUT_FILENO, "\n", 1);
    if (ret < 0)
        return EOF;
    len += ret;

    return len;
}

char* gets(char* buf)
{
    int n = read(STDIN_FILENO, buf, __SIZE_MAX__);
    if (n > 0) {
      if (buf[n-1] == '\n')
        buf[n-1] = 0;
      else
        buf[n] = 0;
      return buf;
    }
    return NULL;
}
