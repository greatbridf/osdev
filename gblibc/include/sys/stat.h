#ifndef __GBLIBC_SYS_STAT_H
#define __GBLIBC_SYS_STAT_H

#include <bits/alltypes.h>
#include <sys/types.h>

#ifdef __cplusplus
extern "C" {
#endif

struct stat {
    dev_t st_dev;
    ino_t st_ino;
    mode_t st_mode;
    uint16_t st_nlink;
    uint16_t st_uid;
    uint16_t st_gid;
    dev_t st_rdev;
    off_t st_size;
    blksize_t st_blksize;
    blkcnt_t st_blocks;

    struct timespec st_atim;
    struct timespec st_mtim;
    struct timespec st_ctim;
};

int stat(const char* pathname, struct stat* statbuf);
int fstat(int fd, struct stat* statbuf);

mode_t umask(mode_t mask);

#ifdef __cplusplus
}
#endif

#endif
