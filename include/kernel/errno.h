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

#define EPERM 1
#define ENOENT 2
#define ESRCH 3
#define EINTR 4
#define EBADF 9
#define ECHILD 10
#define ENOMEM 12
#define EACCES 13
#define EEXIST 17
#define ENOTDIR 20
#define EISDIR 21
#define EINVAL 22
#define ENOTTY 25
#define EPIPE 32

// non-standard errors
#define ENOTFOUND 200

#endif
