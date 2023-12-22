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
typedef unsigned uid_t;
typedef unsigned gid_t;
typedef unsigned mode_t;
typedef unsigned long nlink_t;

typedef uint64_t ino64_t;
typedef int64_t off64_t;

#ifdef __cplusplus
}
#endif

#endif
