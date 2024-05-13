#pragma once

#include <stdint.h>
#include <sys/types.h>

namespace fs {

class vfs;

struct inode {
    ino_t ino;
    vfs* fs;
    size_t size;

    nlink_t nlink;

    mode_t mode;
    uid_t uid;
    gid_t gid;
};

} // namespace fs
