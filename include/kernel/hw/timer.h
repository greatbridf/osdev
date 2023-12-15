#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

void init_pit(void);

void inc_tick(void);

size_t current_ticks(void);

#ifdef __cplusplus
}
#endif
