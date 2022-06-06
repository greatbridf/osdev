#include <types/bitmap.h>

#define SZ (8)

size_t make_bm_size(size_t n)
{
    return sizeof(size_t) + (n / SZ) + ((n % SZ) ? 1 : 0);
}

int bm_test(char* bm, size_t n)
{
    return (bm[n / SZ] & (1 << (n % SZ))) != 0;
}

void bm_set(char* bm, size_t n)
{
    bm[n / SZ] |= (1 << (n % SZ));
}
void bm_clear(char* bm, size_t n)
{
    bm[n / SZ] &= (~(1 << (n % SZ)));
}
