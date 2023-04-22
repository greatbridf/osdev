#include <devutil.h>
#include <stdint.h>

static inline uint64_t do_div(uint64_t a, uint64_t b, uint64_t* remainder)
{
    uint64_t r = 0, q = 0;
    for (int32_t i = 0; i < 64; i++) {
        r = (r << 1) + (a >> 63);
        a <<= 1;
        q <<= 1;
        if (r >= b) {
            r -= b;
            q += 1;
        }
    }
    if (remainder)
        *remainder = r;
    return q;
}

static inline int64_t do_div_s(int64_t a, int64_t b, uint64_t* remainder)
{
    int32_t qf = 0, rf = 0;
    if (a < 0) {
        qf = rf = 1;
        a = -a;
    }
    if (b < 0) {
        qf ^= 1;
        b = -b;
    }

    int64_t quotient = do_div(a, b, (uint64_t*)remainder);

    if (qf)
        quotient = -quotient;
    if (remainder && rf)
        *remainder = -*remainder;

    return quotient;
}

int64_t __divdi3(int64_t a, int64_t b)
{
    return do_div_s(a, b, (uint64_t*)0);
}

int64_t __moddi3(int64_t a, int64_t b)
{
    uint64_t remainder = 0;
    do_div_s(a, b, &remainder);
    return remainder;
}

uint64_t __udivdi3(uint64_t a, uint64_t b)
{
    return do_div(a, b, NULL);
}

uint64_t __umoddi3(uint64_t a, uint64_t b)
{
    uint64_t rem = 0;
    do_div(a, b, &rem);
    return rem;
}
