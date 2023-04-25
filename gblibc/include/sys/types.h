#ifndef __GBLIBC_SYS_TYPES_H
#define __GBLIBC_SYS_TYPES_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef int pid_t;
typedef uint32_t ino_t;
typedef int32_t off_t;
typedef uint32_t dev_t;

#define INVALID_DEVICE (~(dev_t)0)

#ifdef __cplusplus
}
#endif

#endif
