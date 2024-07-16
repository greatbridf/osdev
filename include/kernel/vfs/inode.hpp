#pragma once

#include <bits/alltypes.h>
#include <stdint.h>
#include <sys/types.h>

#include <kernel/vfs/vfsfwd.hpp>

namespace fs {

struct inode {
    ino_t ino {};
    size_t size {};
    nlink_t nlink {};

    vfs* fs {};
    void* fs_data {};

    struct timespec atime {};
    struct timespec ctime {};
    struct timespec mtime {};

    mode_t mode {};
    uid_t uid {};
    gid_t gid {};
};

} // namespace fs
