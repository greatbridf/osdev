#ifndef __GBLIBC_STDIO_H_
#define __GBLIBC_STDIO_H_

#include <stdint.h>

#undef EOF
#define EOF (-1)

#ifdef __cplusplus
extern "C" {
#endif

int snprintf(char* buf, size_t bufsize, const char* fmt, ...);

#ifdef __cplusplus
}
#endif

#endif
