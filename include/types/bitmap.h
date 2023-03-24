#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

int bm_test(uint8_t* bm, size_t n);
void bm_set(uint8_t* bm, size_t n);
void bm_clear(uint8_t* bm, size_t n);

#ifdef __cplusplus
}
#endif
