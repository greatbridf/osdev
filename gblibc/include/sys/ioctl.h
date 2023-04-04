#ifndef __GBLIBC_SYS_IOCTL_H_
#define __GBLIBC_SYS_IOCTL_H_

#include <bits/ioctl.h>

#ifdef __cplusplus
extern "C" {
#endif

int ioctl(int fd, unsigned long request, ...);

#ifdef __cplusplus
}
#endif

#endif
