#ifndef __GBLIBC_BITS_ALLTYPES_H_
#define __GBLIBC_BITS_ALLTYPES_H_

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef size_t blksize_t;
typedef size_t blkcnt_t;

struct timespec {
    time_t tv_sec;
    long tv_nsec;
};

struct timeval {
    time_t tv_sec;
    size_t tv_usec;
};

struct timezone {
    int tz_minuteswest;
    int tz_dsttime;
};

#ifdef __cplusplus
}
#endif

#endif
