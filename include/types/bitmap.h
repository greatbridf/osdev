#pragma once

#include <types/stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

int bm_test(char* bm, size_t n);
void bm_set(char* bm, size_t n);
void bm_clear(char* bm, size_t n);

#ifdef __cplusplus
}
#endif
