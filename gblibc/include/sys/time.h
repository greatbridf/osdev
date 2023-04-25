#ifndef __GBLIBC_SYS_TIME_H_
#define __GBLIBC_SYS_TIME_H_

#include <bits/alltypes.h>

#ifdef __cplusplus
extern "C" {
#endif

int gettimeofday(struct timeval* tv, struct timezone* tz);

#ifdef __cplusplus
}
#endif

#endif
