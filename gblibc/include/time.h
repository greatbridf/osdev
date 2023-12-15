#ifndef __GBLIBC_TIME_H_
#define __GBLIBC_TIME_H_

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define CLOCK_REALTIME 0

typedef int clockid_t;

struct timespec {
    time_t tv_sec;
    long tv_nsec;
    int : 32; // padding
};

#ifdef __cplusplus
}
#endif

#endif
