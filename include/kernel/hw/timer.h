#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

struct user_timeval {
    time_t tv_sec;
    size_t tv_usec;
};

void init_pit(void);

void inc_tick(void);

time_t current_ticks(void);

#ifdef __cplusplus
}
#endif
