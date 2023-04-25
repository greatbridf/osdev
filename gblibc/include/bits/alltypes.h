#ifndef __GBLIBC_BITS_ALLTYPES_H_
#define __GBLIBC_BITS_ALLTYPES_H_

#include <time.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef size_t blksize_t;
typedef size_t blkcnt_t;

struct timespec {
    time_t tv_sec;
    long tv_nsec;
};

#ifdef __cplusplus
}
#endif

#endif
