#include <types/bitmap.h>

#define B_TYPE BITMAP_UNDERLYING_TYPE
#define SZ (sizeof(B_TYPE) * 8)

size_t make_bm_size(size_t n)
{
    return sizeof(size_t) + (n / SZ) + ((n % SZ) ? 1 : 0);
}

int bm_test(struct bitmap* bm, size_t n)
{
    return (bm->v[n / SZ] & (1 << (n % SZ))) != 0;
}

void bm_set(struct bitmap* bm, size_t n)
{
    bm->v[n / SZ] |= (1 << (n % SZ));
}
void bm_clear(struct bitmap* bm, size_t n)
{
    bm->v[n / SZ] &= (~(1 << (n % SZ)));
}
