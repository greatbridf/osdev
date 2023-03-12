#ifndef __GBLIBC_STRING_H_
#define __GBLIBC_STRING_H_

#include <stdint.h>

#undef CR
#undef LF
#define CR ('\r')
#define LF ('\n')

#ifdef __cplusplus
extern "C" {
#endif

void* memcpy(void* dst, const void* src, size_t n);
void* memset(void* dst, int c, size_t n);

int strcmp(const char* s1, const char* s2);
size_t strlen(const char* str);

char* strncpy(char* dst, const char* src, size_t n);

#ifdef __cplusplus
}
#endif

#endif
