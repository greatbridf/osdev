#ifndef __GBLIBC_ERRNO_H_
#define __GBLIBC_ERRNO_H_

#ifdef __cplusplus
extern "C" {
#endif

extern int* __errno_location(void);

#undef errno
#define errno (*__errno_location())

#define EPERM 1
#define ENOENT 2
#define ESRCH 3
#define EINTR 4
#define EIO 5
#define EBADF 9
#define ECHILD 10
#define ENOMEM 12
#define EACCES 13
#define EFAULT 14
#define EEXIST 17
#define ENODEV 19
#define ENOTDIR 20
#define EISDIR 21
#define EINVAL 22
#define ENOTTY 25
#define ESPIPE 29
#define EROFS 30
#define EPIPE 32
#define ELOOP 40

#ifdef __cplusplus
}
#endif

#endif
