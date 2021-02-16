#include <kernel/stdio.h>

#include <types/stdint.h>

// where n is in the range of [0, 9]
static inline char d_to_c(int32_t n)
{
    return '0' + n;
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

size_t
snprint_decimal(
    char* buf,
    size_t buf_size,
    int32_t num)
{
    size_t n_write = 0;

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

static inline size_t
snprint_char(
    char* buf,
    size_t buf_size,
    char c)
{
    if (buf_size > 1)
        *buf = c;
    return sizeof(c);
}

size_t
snprintf(
    char* buf,
    size_t buf_size,
    const char* fmt,
    ...)
{
    size_t n_write = 0;
    void* arg_ptr = ((void*)&buf) + sizeof(char*) + sizeof(size_t) + sizeof(const char*);

    for (char c; (c = *fmt) != 0x00; ++fmt) {
        if (c == '%') {
            size_t n_tmp_write = 0;

            switch (*(++fmt)) {

            // int32 decimal
            case 'd':
                n_tmp_write = snprint_decimal(buf, buf_size, *(int32_t*)arg_ptr);
                arg_ptr += sizeof(int32_t);
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
