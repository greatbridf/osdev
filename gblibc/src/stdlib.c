#include <alloca.h>
#include <priv-vars.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <syscall.h>
#include <unistd.h>
#include <string.h>

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

static inline void _cut_block(struct mem* p, size_t mem_size, size_t block_size)
{
    if (block_size >= mem_size + sizeof(struct mem) + MINIMUM_ALLOCATION_SIZE) {
        p->sz = mem_size;

        struct mem* next = _next(p, mem_size);
        next->flag = 0;
        next->sz = block_size - mem_size - sizeof(struct mem);
    }
}

void* malloc(size_t size)
{
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
    _cut_block(p, size, sz);

    return _next(p, 0);
}

void* realloc(void* ptr, size_t newsize)
{
    if (!ptr)
        return malloc(newsize);

    struct mem* p = ptr - sizeof(struct mem);
    size_t oldsize = p->sz;

    _union(p);
    if (_is_end(p)) {
        if (_size(p) < newsize + sizeof(struct mem))
            sbrk(_max(128 * 1024, newsize + sizeof(struct mem)));

        p->sz = newsize;
        struct mem* next = _next(p, newsize);
        next->flag = 0;
        next->sz = 0;

        return ptr;
    }

    if (p->sz >= newsize) {
        _cut_block(p, newsize, p->sz);
        return ptr;
    }

    void* newptr = malloc(newsize);
    if (!newptr)
        return NULL;
    
    memcpy(newptr, ptr, oldsize);
    free(ptr);
    return newptr;
}

void free(void* ptr)
{
    struct mem* p = ptr - sizeof(struct mem);
    p->flag &= ~MEM_ALLOCATED;
    _union(p);
}

static inline void _swap(void* a, void* b, size_t sz)
{
    void* tmp = alloca(sz);
    memcpy(tmp, a, sz);
    memcpy(a, b, sz);
    memcpy(b, tmp, sz);
}

void qsort(void* arr, size_t len, size_t sz, comparator_t cmp) {
    if (len <= 1)
        return;

    char* pivot = alloca(sz);
    memcpy(pivot, arr + sz * (rand() % len), sz);

    int i = 0, j = 0, k = len;
    while (i < k) {
        int res = cmp(arr + sz * i, pivot);
        if (res < 0)
            _swap(arr + sz * i++, arr + sz * j++, sz);
        else if (res > 0)
            _swap(arr + sz * i, arr + sz * --k, sz);
        else
            i++;
    }

    qsort(arr, j, sz, cmp);
    qsort(arr + sz * k, len - k, sz, cmp);
}

static unsigned int __next_rand;
int rand(void)
{
    return rand_r(&__next_rand);
}

int rand_r(unsigned int* seedp)
{
    *seedp = *seedp * 1103515245 + 12345;
    return (unsigned int) (*seedp / 65536) % 32768;
}

void srand(unsigned int seed)
{
    __next_rand = seed;
    rand();
}

void* bsearch(const void* key, const void* base, size_t num, size_t size, comparator_t cmp)
{
    if (num == 0)
        return NULL;

    size_t mid = num / 2;
    int result = cmp(key, base + size * mid);

    if (result == 0)
        return (void*)base + size * mid;
    
    if (result > 0) {
        ++mid;
        return bsearch(key, base + size * mid, num - mid, size, cmp);
    }
    
    return bsearch(key, base, mid, size, cmp);
}

int setenv(const char* name, const char* value, int overwrite)
{
    size_t i = 0;
    for (; environ[i]; ++i) {
        char* eqpos = strchr(environ[i], '=');
        if (strncmp(name, environ[i], eqpos - environ[i]) == 0) {
            if (overwrite)
                goto fill_p;
            return 0;
        }
    }

    if (i + 2 == environ_size) {
        environ_size *= 2;
        char** newarr = malloc(environ_size * sizeof(char*));
        if (!newarr)
            return -1;
        
        memcpy(newarr, environ, sizeof(char*) * environ_size / 2);
        free(environ);
        environ = newarr;
    }
    environ[i + 1] = NULL;

fill_p:;
    char* newenv = NULL;
    if (asprintf(&newenv, "%s=%s", name, value) < 0)
        return -1;

    environ[i] = newenv;
    return 0;
}
