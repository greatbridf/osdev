#pragma once

#include <types/stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

void init_pit(void);

void inc_tick(void);

time_t current_ticks(void);

#ifdef __cplusplus
}
#endif
