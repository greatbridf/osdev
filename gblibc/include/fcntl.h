#ifndef __GBLIBC_FCNTL_H_
#define __GBLIBC_FCNTL_H_

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

int open(const char* filename, int flags, ...);

#ifdef __cplusplus
}
#endif

#endif
