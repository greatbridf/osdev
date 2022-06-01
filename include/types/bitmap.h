#pragma once

#include <types/stdint.h>

#define BITMAP_UNDERLYING_TYPE char

struct bitmap {
    size_t size;
    BITMAP_UNDERLYING_TYPE v[];
};

size_t make_bm_size(size_t n);

int bm_test(struct bitmap* bm, size_t n);
void bm_set(struct bitmap* bm, size_t n);
void bm_clear(struct bitmap* bm, size_t n);
