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

char* strerror(int errnum);

int strcmp(const char* s1, const char* s2);
int strncmp(const char* s1, const char* s2, size_t n);
int strcasecmp(const char* s1, const char* s2);
int strncasecmp(const char* s1, const char* s2, size_t n);
size_t strlen(const char* str);
char* strchr(const char* str, int character);
char* strrchr(const char* str, int character);
char* strchrnul(const char* str, int character);
size_t strcspn(const char* str1, const char* str2);
char* strstr(const char* str1, const char* str2);
char* strpbrk(const char* str1, const char* str2);

char* strcpy(char* dst, const char* src);
char* strncpy(char* dst, const char* src, size_t n);
char* stpcpy(char* dst, const char* src);
char* stpncpy(char* dst, const char* src, size_t n);

char* strdup(const char* str);
char* strndup(const char* str, size_t n);

#ifdef __cplusplus
}
#endif

#endif
