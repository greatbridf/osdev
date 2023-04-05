#ifndef __GBOS_ERRNO_H
#define __GBOS_ERRNO_H

#include <types/types.h>

#ifdef __cplusplus
extern "C" {
#endif

extern uint32_t* _get_errno(void);

#define errno (*_get_errno())

#ifdef __cplusplus
}
#endif

#define ENOMEM (1 << 0)
#define EEXIST (1 << 1)
#define ENOENT (1 << 2)
#define EINVAL (1 << 3)
#define EISDIR (1 << 4)
#define ENOTDIR (1 << 5)
#define ENOTFOUND (1 << 6)
#define ECHILD (1 << 7)
#define EBADF (1 << 8)
#define EPERM (1 << 9)
#define ESRCH (1 << 10)
#define EINTR (1 << 11)
#define EPIPE (1 << 12)
#define ENOTTY (1 << 13)

#endif
