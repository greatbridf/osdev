#ifndef __GBLIBC_STDLIB_H_
#define __GBLIBC_STDLIB_H_

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

int atoi(const char* str);

void __attribute__((noreturn)) exit(int status);

void* malloc(size_t size);
void free(void* ptr);

#ifdef __cplusplus
}
#endif

#endif
