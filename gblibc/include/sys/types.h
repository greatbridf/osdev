#ifndef __GBLIBC_SYS_TYPES_H
#define __GBLIBC_SYS_TYPES_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef int pid_t;
typedef unsigned long ino_t;
typedef long off_t;
typedef unsigned dev_t;
typedef unsigned uid_t;
typedef unsigned gid_t;
typedef unsigned short mode_t;
typedef unsigned long nlink_t;

typedef unsigned long long ino64_t;
typedef long long off64_t;

typedef off64_t loff_t;

#ifdef __cplusplus
}
#endif

#endif
