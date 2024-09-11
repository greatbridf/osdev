#include <assert.h>

#include <kernel/vfs.hpp>
#include <kernel/vfs/inode.hpp>

using namespace fs;

int vfs::statx(inode* ind, struct statx* st, unsigned int mask) {
    st->stx_mask = 0;

    if (mask & STATX_NLINK) {
        st->stx_nlink = ind->nlink;
        st->stx_mask |= STATX_NLINK;
    }

    if (mask & STATX_ATIME) {
        st->stx_atime.tv_nsec = ind->atime.tv_nsec;
        st->stx_atime.tv_sec = ind->atime.tv_sec;

        st->stx_mask |= STATX_ATIME;
    }

    if (mask & STATX_CTIME) {
        st->stx_ctime.tv_nsec = ind->ctime.tv_nsec;
        st->stx_ctime.tv_sec = ind->ctime.tv_sec;

        st->stx_mask |= STATX_CTIME;
    }

    if (mask & STATX_MTIME) {
        st->stx_mtime.tv_nsec = ind->mtime.tv_nsec;
        st->stx_mtime.tv_sec = ind->mtime.tv_sec;

        st->stx_mask |= STATX_MTIME;
    }

    if (mask & STATX_SIZE) {
        st->stx_size = ind->size;
        st->stx_mask |= STATX_SIZE;
    }

    st->stx_mode = 0;
    if (mask & STATX_MODE) {
        st->stx_mode |= ind->mode & ~S_IFMT;
        st->stx_mask |= STATX_MODE;
    }

    if (mask & STATX_TYPE) {
        st->stx_mode |= ind->mode & S_IFMT;
        if (S_ISBLK(ind->mode) || S_ISCHR(ind->mode)) {
            auto dev = i_device(ind);
            assert(!(dev & 0x80000000));

            st->stx_rdev_major = NODE_MAJOR(dev);
            st->stx_rdev_minor = NODE_MINOR(dev);
        }
        st->stx_mask |= STATX_TYPE;
    }

    if (mask & STATX_INO) {
        st->stx_ino = ind->ino;
        st->stx_mask |= STATX_INO;
    }

    if (mask & STATX_BLOCKS) {
        st->stx_blocks = (ind->size + 512 - 1) / 512;
        st->stx_blksize = io_blksize();
        st->stx_mask |= STATX_BLOCKS;
    }

    if (mask & STATX_UID) {
        st->stx_uid = ind->uid;
        st->stx_mask |= STATX_UID;
    }

    if (mask & STATX_GID) {
        st->stx_gid = ind->gid;
        st->stx_mask |= STATX_GID;
    }

    st->stx_dev_major = NODE_MAJOR(fs_device());
    st->stx_dev_minor = NODE_MINOR(fs_device());

    // TODO: support more attributes
    st->stx_attributes_mask = 0;

    return 0;
}

dev_t vfs::i_device(inode*) {
    return -ENODEV;
}
