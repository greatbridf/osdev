#ifndef __GBLIBC_FCNTL_H_
#define __GBLIBC_FCNTL_H_

#include <stdint.h>

#define O_RDONLY          00
#define O_WRONLY          01
#define O_RDWR            02
#define O_CREAT         0100
#define O_EXCL          0200
#define O_TRUNC        01000
#define O_APPEND       02000
#define O_DIRECTORY  0200000
#define O_CLOEXEC   02000000

#define F_DUPFD 0
#define F_GETFD 1
#define F_SETFD 2
#define F_GETFL 3
#define F_SETFL 4
#define F_DUPFD_CLOEXEC 1030
// TODO: more flags

#define FD_CLOEXEC 1

#define AT_FDCWD (-100)
#define AT_SYMLINK_NOFOLLOW   0x100
#define AT_REMOVEDIR          0x200
#define AT_SYMLINK_FOLLOW     0x400
#define AT_STATX_SYNC_AS_STAT 0x0000
#define AT_STATX_SYNC_TYPE    0x6000
#define AT_STATX_SYNC_FORCE   0x8000
#define AT_STATX_DONT_SYNC    0x2000

#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

#ifdef __cplusplus
extern "C" {
#endif

int open(const char* filename, int flags, ...);

#ifdef __cplusplus
}
#endif

#endif
