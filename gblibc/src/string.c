#include <ctype.h>
#include <errno.h>
#include <stdint.h>
#include <stdlib.h>

#define BYTES_PER_MAX_COPY_UNIT (sizeof(uint32_t) / sizeof(uint8_t))

int memcmp(const void* ptr1, const void* ptr2, size_t num)
{
    while (num--) {
        if (*(const char*)ptr1 < *(const char*)ptr2)
            return -1;
        else if (*(const char*)ptr1 > *(const char*)ptr2)
            return 1;
    }
    return 0;
}

void* memmove(void* dst, const void* src, size_t n)
{
    void* orig_dst = dst;
    while (n--)
        *(char*)(dst++) = *(const char*)(src++);
    return orig_dst;
}

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

void* mempcpy(void* dst, const void* src, size_t n)
{
    return memcpy(dst, src, n) + n;
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

char* strchr(const char* str, int c)
{
    const char* p = str;
    while (*p) {
        if (*p == c)
            return (char*)p;
        ++p;
    }
    return NULL;
}

char* strrchr(const char* str, int c)
{
    const char* p = str + strlen(str) - 1;
    while (p >= str) {
        if (*p == c)
            return (char*)p;
        --p;
    }
    return NULL;
}

char* strchrnul(const char* str, int c)
{
    char* ret = strchr(str, c);
    if (ret)
        return ret;
    return (char*)str + strlen(str);
}

char* strcpy(char* dst, const char* src)
{
    return memcpy(dst, src, strlen(src) + 1);
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

char* stpcpy(char* restrict dst, const char* restrict src)
{
    return memcpy(dst, src, strlen(src) + 1) + strlen(src);
}

char* stpncpy(char* restrict dst, const char* restrict src, size_t n)
{
    size_t len = strlen(src);

    if (len < n) {
        memset(dst + len, 0x00, n - len);
        memcpy(dst, src, len);
    } else {
        memcpy(dst, src, n);
    }

    return dst + len;
}

int strncmp(const char* s1, const char* s2, size_t n)
{
    if (n == 0)
        return 0;

    int c;
    while (n-- && (c = *s1 - *s2) == 0 && *s1) {
        ++s1;
        ++s2;
    }
    return c;
}

int strcmp(const char* s1, const char* s2)
{
    return strncmp(s1, s2, __SIZE_MAX__);
}

int strncasecmp(const char* s1, const char* s2, size_t n)
{
    if (n == 0)
        return 0;

    int c;
    while (n-- && (c = tolower(*s1) - tolower(*s2)) == 0 && *s1) {
        ++s1;
        ++s2;
    }
    return c;
}

int strcasecmp(const char* s1, const char* s2)
{
    return strncasecmp(s1, s2, __SIZE_MAX__);
}

size_t strcspn(const char* str1, const char* str2)
{
    size_t ret = 0;
    while (*str1) {
        ++ret;
        for (const char* p = str2; *p; ++p) {
            if (*str1 == *p)
                return ret;
        }
        ++str1;
    }
    return ret;
}

char* strstr(const char* str1, const char* str2)
{
    const char* p = str1;

    while (*p) {
        if (*p != *str2) {
            ++p;
            continue;
        }

        const char* p1 = p;
        const char* q = str2;
        while (*q) {
            if (*p1 != *q)
                break;
            ++p1;
            ++q;
        }
        if (!*q)
            break;
        p = p1;
    }

    if (*p)
        return (char*)p;
    return NULL;
}

char* strpbrk(const char* str1, const char* str2)
{
    size_t n = strcspn(str1, str2);
    if (str1[n])
        return (char*)str1 + n;
    return NULL;
}

char* strerror(int errnum)
{
    switch (errnum) {
    case EPERM:
        return "Operation not permitted";
    case ENOENT:
        return "No such file or directory";
    case ESRCH:
        return "No such process";
    case EINTR:
        return "Interrupted system call";
    case EBADF:
        return "Bad file descriptor";
    case ECHILD:
        return "No child process";
    case ENOMEM:
        return "Out of memory";
    case EEXIST:
        return "File exists";
    case ENOTDIR:
        return "Not a directory";
    case EISDIR:
        return "Is a directory";
    case EINVAL:
        return "Invalid argument";
    case ENOTTY:
        return "Not a tty";
    case EPIPE:
        return "Broken pipe";
    default:
        return "No error information";
    }
}

char* strndup(const char* str, size_t n)
{
    size_t len = strlen(str);
    if (len > n)
        len = n;
    char* ret = malloc(len + 1);
    if (!ret)
        return NULL;
    
    memcpy(ret, str, len);
    ret[len] = 0;
    return ret;
}

char* strdup(const char* str)
{
    return strndup(str, __SIZE_MAX__);
}
