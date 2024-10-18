#ifndef __GBLIBC_TIME_H_
#define __GBLIBC_TIME_H_

#include <stdint.h>
#include <bits/alltypes.h>

#ifdef __cplusplus
extern "C" {
#endif

#define CLOCK_REALTIME 0
#define CLOCK_MONOTONIC 1
typedef int clockid_t;

#ifdef __cplusplus
}
#endif

#endif
