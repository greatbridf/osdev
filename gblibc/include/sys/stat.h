#ifndef __GBLIBC_SYS_STAT_H
#define __GBLIBC_SYS_STAT_H

#include <stdint.h>

#define STATX_TYPE (1 << 0)
#define STATX_MODE (1 << 1)
#define STATX_NLINK (1 << 2)
#define STATX_UID (1 << 3)
#define STATX_GID (1 << 4)
#define STATX_ATIME (1 << 5)
#define STATX_MTIME (1 << 6)
#define STATX_CTIME (1 << 7)
#define STATX_INO (1 << 8)
#define STATX_SIZE (1 << 9)
#define STATX_BLOCKS (1 << 10)
#define STATX_BASIC_STATS (0x7ff)
#define STATX_BTIME (1 << 11)

#ifdef __cplusplus
extern "C" {
#endif

struct statx_timestamp {
    int64_t tv_sec;
    uint32_t tv_nsec;
    int32_t __reserved;
};

struct statx {
    uint32_t stx_mask;
    uint32_t stx_blksize;
    uint64_t stx_attributes;
    uint32_t stx_nlink;
    uint32_t stx_uid;
    uint32_t stx_gid;
    uint16_t stx_mode;
    uint16_t __spare0[1];
    uint64_t stx_ino;
    uint64_t stx_size;
    uint64_t stx_blocks;
    uint64_t stx_attributes_mask;
    struct statx_timestamp stx_atime;
    struct statx_timestamp stx_btime;
    struct statx_timestamp stx_ctime;
    struct statx_timestamp stx_mtime;
    uint32_t stx_rdev_major;
    uint32_t stx_rdev_minor;
    uint32_t stx_dev_major;
    uint32_t stx_dev_minor;
    uint64_t stx_mnt_id;
    uint64_t stx_dio_alignment[13];
};

#ifdef __cplusplus
}
#endif

#endif
