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

int memcmp(const void* ptr1, const void* ptr2, size_t num);

void* memmove(void* dst, const void* src, size_t n);
void* memcpy(void* dst, const void* src, size_t n);
void* mempcpy(void* dst, const void* src, size_t n);
void* memset(void* dst, int c, size_t n);

int strcmp(const char* s1, const char* s2);
size_t strlen(const char* str);
char* strchr(const char* str, int character);
char* strrchr(const char* str, int character);
char* strchrnul(const char* str, int character);

char* strcpy(char* dst, const char* src);
char* strncpy(char* dst, const char* src, size_t n);
char* stpcpy(char* dst, const char* src);
char* stpncpy(char* dst, const char* src, size_t n);

#ifdef __cplusplus
}
#endif

#endif
