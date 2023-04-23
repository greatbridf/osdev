#include <assert.h>
#include <devutil.h>
#include <fcntl.h>
#include <list.h>
#include <stdint.h>
#include <stdio.h>
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <priv-vars.h>

static inline int __feof_or_error(FILE* stream)
{
    return !!(stream->flags & (FILE_ERROR | FILE_EOF));
}

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
    return fputs(str, stdout);
}

char* gets(char* buf)
{
    int c, num = 0;
    while ((c = getchar()) != EOF && c != '\n')
        buf[num++] = c;
    buf[num] = 0;

    if (c == EOF)
        return NULL;
    return buf;
}

int vfprintf_u32(uint32_t num, FILE* stream)
{
    if (num <= 9) {
        fputc(d_to_c(num), stream);
        return 1;
    }

    int ret = vfprintf_u32(num / 10, stream);
    fputc(d_to_c(num % 10), stream);
    return ret + 1;
}

int vfprintf_d32(int32_t num, FILE* stream)
{
    if (num < 0) {
        fputc('-', stream);
        return vfprintf_u32(-num, stream) + 1;
    }
    return vfprintf_u32(num, stream);
}

int vfprintf_u64(uint64_t num, FILE* stream)
{
    if (num <= 9) {
        fputc(d_to_c(num), stream);
        return 1;
    }

    int ret = vfprintf_u64(num / 10, stream);
    fputc(d_to_c(num % 10), stream);
    return ret + 1;
}

int vfprintf_d64(int64_t num, FILE* stream)
{
    if (num < 0) {
        fputc('-', stream);
        return vfprintf_u64(-num, stream) + 1;
    }
    return vfprintf_u64(num, stream);
}

int vfprintf_x32(uint32_t num, int off, FILE* stream)
{
    // print leading 0x
    if (off & 1) {
        --off;
        fputc('0', stream);
        fputc('X' + off, stream);
        return vfprintf_x32(num, off, stream) + 2;
    }

    if (num <= 15) {
        fputc(X_to_c(num) + off, stream);
        return 1;
    }

    int ret = vfprintf_x32(num >> 4, off, stream);
    fputc(X_to_c(num & 0xf) + off, stream);
    return ret + 1;
}

int vfprintf_x64(uint64_t num, int off, FILE* stream)
{
    // print leading 0x
    if (off & 1) {
        --off;
        fputc('0', stream);
        fputc('X' + off, stream);
        return vfprintf_x64(num, off, stream) + 2;
    }

    if (num <= 15) {
        fputc(X_to_c(num) + off, stream);
        return 1;
    }

    int ret = vfprintf_x64(num >> 4, off, stream);
    fputc(X_to_c(num & 0xf) + off, stream);
    return ret + 1;
}

int vfprintf(FILE* stream, const char* fmt, va_list args)
{
    int n = 0;

    for (char c = 0; (c = *fmt) != 0x00; ++fmt) {
        if (c == '%') {
            switch (*(++fmt)) {

            // int
            case 'd':
                n += vfprintf_d32(va_arg(args, int), stream);
                break;

            case 'x':
                n += vfprintf_x32(va_arg(args, unsigned int), 'a' - 'A' + 1, stream);
                break;

            case 'X':
                n += vfprintf_x32(va_arg(args, unsigned int), 1, stream);
                break;

            // long decimal
            case 'l':
                switch (*(++fmt)) {
                // long long aka int64
                case 'l':
                    switch (*(++fmt)) {
                    case 'd':
                        n += vfprintf_d64(va_arg(args, long long), stream);
                        break;
                    case 'x':
                        n += vfprintf_x64(va_arg(args, unsigned long long), 'a' - 'A' + 1, stream);
                        break;
                    case 'X':
                        n += vfprintf_x64(va_arg(args, unsigned long long), 'a' - 'A' + 1, stream);
                        break;
                    }
                    break;
                // long int aka int32
                case 'd':
                    n += vfprintf_d32(va_arg(args, int), stream);
                    break;
                case 'x':
                    n += vfprintf_x32(va_arg(args, unsigned int), 'a' - 'A' + 1, stream);
                    break;

                case 'X':
                    n += vfprintf_x32(va_arg(args, unsigned int), 1, stream);
                    break;
                }
                break;

            // c string
            case 's':
                n += fprintf(stream, va_arg(args, const char*));
                break;

            // int8 char
            case 'c':
                ++n;
                fputc(va_arg(args, int), stream);
                break;

            // pointer
            case 'p':
#ifdef __32bit_system
                n += vfprintf_x32(va_arg(args, size_t), 'a' - 'A' + 1, stream);
#else
                n += vfprintf_x64(va_arg(args, size_t), 'a' - 'A' + 1, stream);
#endif
                break;

            default:
                ++n;
                fputc(*(fmt - 1), stream);
                break;
            }
        } else {
            ++n;
            fputc(c, stream);
        }
    }

    return n;
}

int fprintf(FILE* stream, const char* fmt, ...)
{
    va_list args;
    va_start(args, fmt);

    int ret = vfprintf(stream, fmt, args);

    va_end(args);
    return ret;
}

int vprintf(const char* fmt, va_list args)
{
    return vfprintf(stdout, fmt, args);
}

int printf(const char* fmt, ...)
{
    va_list args;
    va_start(args, fmt);

    int ret = vprintf(fmt, args);

    va_end(args);
    return ret;
}

int putchar(int c)
{
    fputc(c, stdout);
    return c;
}

FILE* fopen(const char* path, const char* mode)
{
    uint32_t flags = 0, file_flags = 0;

    if (strcmp(mode, "r") == 0)
        flags = O_RDONLY, file_flags = FILE_READ;

    if (strcmp(mode, "r+") == 0)
        flags = O_RDWR, file_flags = FILE_READ | FILE_WRITE;

    if (strcmp(mode, "w") == 0)
        flags = O_WRONLY | O_CREAT | O_TRUNC, file_flags = FILE_WRITE;

    if (strcmp(mode, "w+") == 0)
        flags = O_RDWR | O_CREAT | O_TRUNC, file_flags = FILE_READ | FILE_WRITE;
    
    assert(flags);

    int fd = open(path, flags, 0644);
    if (fd < 0)
        goto open_fail;
    
    FILE* file = malloc(sizeof(FILE));
    if (!file)
        goto file_malloc_fail;
    
    file->fd = fd;
    file->flags = file_flags;

    if (file_flags & FILE_READ) {
        file->rbuf = malloc(BUFSIZ);
        if (!file->rbuf)
            goto rbuf_malloc_fail;
        file->rbsz = BUFSIZ;
    }

    if (file_flags & FILE_WRITE) {
        file->wbuf = malloc(BUFSIZ);
        if (!file->wbuf)
            goto wbuf_malloc_fail;
        file->wbsz = BUFSIZ;
    }

    return file;

wbuf_malloc_fail:
    free(file->rbuf);

rbuf_malloc_fail:
    free(file);

file_malloc_fail:
    close(fd);

open_fail:
    return NULL;
}

int fflush(FILE* stream)
{
    if (__feof_or_error(stream))
        return EOF;

    if (stream->wbuf && stream->wpos) {
        int ret = write(stream->fd, stream->wbuf, stream->wpos);
        if (ret < 0) {
            stream->flags |= FILE_ERROR;
            return EOF;
        }
        if (ret == 0) {
            stream->flags |= FILE_EOF;
            return EOF;
        }
        stream->wpos = 0;
    }

    // TODO: call flush()

    return 0;
}

int fclose(FILE* stream)
{
    if (fflush(stream) == EOF)
        return EOF;
    
    free(stream->rbuf);
    free(stream->wbuf);
    stream->rbsz = 0;
    stream->wbsz = 0;
    
    if (close(stream->fd) < 0)
        return EOF;
    
    NDERASE(NDPTR(stream));
    
    return 0;
}

int fputc_unlocked(int c, FILE* stream)
{
    return putc_unlocked(c, stream);
}

int fputs_unlocked(const char* s, FILE* stream)
{
    // 1 is for the trailing '\n'
    int len = 1;
    for (const char* p = s; *p; ++p, ++len)
        fputc_unlocked(*p, stream);
    fputc_unlocked('\n', stream);
    return len;
}

int fputc(int c, FILE* stream)
{
    // TODO: lock the stream
    return putc_unlocked(c, stream);
}

int fputs(const char* s, FILE* stream)
{
    // TODO: lock the stream
    return fputs_unlocked(s, stream);
}

static inline int __fillbuf(FILE* stream)
{
    if ((stream->rcnt = read(stream->fd, stream->rbuf, stream->rbsz)) >= 2147483648U) {
        stream->rcnt = 0;
        stream->flags |= FILE_ERROR;
        return EOF;
    }
    if (stream->rcnt == 0) {
        stream->flags |= FILE_EOF;
        return EOF;
    }
    stream->rpos = 0;
    return 0;
}

int getc_unlocked(FILE* stream)
{
    if (__feof_or_error(stream))
        return EOF;

    if (stream->rbuf) {
        if (stream->rpos == stream->rcnt) {
            if (__fillbuf(stream) < 0)
                return EOF;
        }
        return stream->rbuf[stream->rpos++];
    } else {
        int c;
        int ret = read(stream->fd, &c, 1);
        if (ret < 0) {
            stream->flags |= FILE_ERROR;
            return EOF;
        }
        if (ret == 0) {
            stream->flags |= FILE_EOF;
            return EOF;
        }
        return c;
    }
}

int putc_unlocked(int c, FILE* stream)
{
    if (__feof_or_error(stream))
        return EOF;

    if (stream->wbuf) {
        stream->wbuf[stream->wpos++] = c;
        if (stream->wpos == stream->wbsz || c == '\n')
            if (fflush(stream) == EOF)
                return EOF;
    } else {
        if (write(stream->fd, &c, 1) < 0) {
            stream->flags |= FILE_ERROR;
            return EOF;
        }
    }

    return c;
}

int getchar(void)
{
    return fgetc(stdin);
}

int fgetc(FILE* stream)
{
    return getc_unlocked(stream);
}

int ferror(FILE* stream)
{
    // TODO: lock the stream
    return ferror_unlocked(stream);
}

int ferror_unlocked(FILE* stream)
{
    return stream->flags & FILE_ERROR;
}

int feof(FILE* stream)
{
    return stream->flags & FILE_EOF;
}

void clearerr(FILE* stream)
{
    stream->flags &= ~FILE_ERROR;
}
