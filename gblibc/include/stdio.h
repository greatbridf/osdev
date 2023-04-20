#ifndef __GBLIBC_STDIO_H_
#define __GBLIBC_STDIO_H_

#include <stdarg.h>
#include <stdint.h>

#undef EOF
#define EOF (-1)

#ifdef __cplusplus
extern "C" {
#endif

int puts(const char* str);
char* gets(char* str);

int vsnprintf(char* buf, size_t bufsize, const char* fmt, va_list args);
int snprintf(char* buf, size_t bufsize, const char* fmt, ...);
int sprintf(char* buf, const char* fmt, ...);

#ifdef __cplusplus
}
#endif

#endif
