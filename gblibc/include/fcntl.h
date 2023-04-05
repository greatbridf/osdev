#ifndef __GBLIBC_FCNTL_H_
#define __GBLIBC_FCNTL_H_

#include <stdint.h>

#define O_CREAT (1 << 0)
#define O_RDONLY (1 << 1)
#define O_WRONLY (1 << 2)
#define O_RDWR (1 << 3)
#define O_DIRECTORY (1 << 4)
#define O_APPEND (1 << 5)
#define O_TRUNC (1 << 6)

#ifdef __cplusplus
extern "C" {
#endif

int open(const char* filename, int flags, ...);

#ifdef __cplusplus
}
#endif

#endif
