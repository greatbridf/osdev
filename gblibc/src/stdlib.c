#include <priv-vars.h>
#include <stdint.h>
#include <stdlib.h>
#include <syscall.h>
#include <unistd.h>

int atoi(const char* str)
{
    int ret = 0;
    while (*str) {
        ret *= 10;
        ret += *str - '0';
    }
    return ret;
}

void __attribute__((noreturn)) exit(int status)
{
    syscall1(SYS_exit, status);
    for (;;)
        ;
}

#define MINIMUM_ALLOCATION_SIZE (8)
#define MEM_ALLOCATED (1)
struct mem {
    uint32_t sz;
    uint32_t flag;
};

static inline void _init_heap(void)
{
    sbrk(128 * 1024);
    struct mem* first = start_brk;
    first->sz = 0;
    first->flag = 0;
}

static inline int _is_end(struct mem* p)
{
    return p->sz == 0;
}

static inline int _is_allocated(struct mem* p)
{
    return !!(p->flag & MEM_ALLOCATED);
}

static inline size_t _max(size_t a, size_t b)
{
    return a > b ? a : b;
}

static inline size_t _size(struct mem* from)
{
    if (!_is_end(from))
        return from->sz;
    
    size_t sz = curr_brk - (void*)from;
    if (sz < sizeof(struct mem))
        return 0;

    return sz - sizeof(struct mem);
}

static inline struct mem* _next(struct mem* p, size_t sz)
{
    return (void*)p + sizeof(struct mem) + sz;
}

static inline void _union(struct mem* p)
{
    if (_is_end(p))
        return;

    for (struct mem* next = _next(p, p->sz);
        !(next->flag & MEM_ALLOCATED);
        next = _next(p, p->sz)) {

        if (_is_end(next)) {
            p->sz = 0;
            break;
        }

        p->sz += sizeof(struct mem);
        p->sz += next->sz;
    }
}

void* malloc(size_t size)
{
    if (curr_brk == start_brk)
        _init_heap();

    if (size < MINIMUM_ALLOCATION_SIZE)
        size = MINIMUM_ALLOCATION_SIZE;

    struct mem* p = start_brk;
    size_t sz = 0;
    for (;; p = _next(p, p->sz)) {
        if (_is_allocated(p))
            continue;

        _union(p);

        sz = _size(p);
        if (_is_end(p)) {
            if (sz < size + sizeof(struct mem))
                sbrk(_max(128 * 1024, size + sizeof(struct mem)));

            sz = p->sz = size;
            struct mem* next = _next(p, size);
            next->flag = 0;
            next->sz = 0;

            break;
        }

        if (sz >= size)
            break;
    }

    p->flag |= MEM_ALLOCATED;

    if (sz >= size + sizeof(struct mem) + MINIMUM_ALLOCATION_SIZE) {
        p->sz = size;

        struct mem* next = _next(p, size);
        next->flag = 0;
        next->sz = sz - size - sizeof(struct mem);
    }

    return _next(p, 0);
}

void free(void* ptr)
{
    struct mem* p = ptr - sizeof(struct mem);
    p->flag &= ~MEM_ALLOCATED;
    _union(p);
}
