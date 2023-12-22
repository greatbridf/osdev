#ifndef __GBLIBC_STDLIB_H_
#define __GBLIBC_STDLIB_H_

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

int atoi(const char* str);

void __attribute__((noreturn)) exit(int status);

void* malloc(size_t size);
void* realloc(void* ptr, size_t newsize);
void free(void* ptr);

typedef int (*comparator_t)(const void* a, const void* b);
void qsort(void* base, size_t num, size_t size, comparator_t comparator);
void* bsearch(
    const void* key,
    const void* base,
    size_t num,
    size_t size,
    comparator_t comparator);

int rand(void);
int rand_r(unsigned int* seedp);
void srand(unsigned int seed);

int setenv(const char* name, const char* value, int overwrite);

#ifdef __cplusplus
}
#endif

#endif
