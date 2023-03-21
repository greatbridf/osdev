#include <fcntl.h>
#include <dirent.h>
#include <string.h>
#include <syscall.h>

DIR* opendir(const char* name)
{
    // TODO: set flags
    int fd = open(name, 0);

    if (fd < 0)
        return NULL;

    return fdopendir(fd);
}

DIR* fdopendir(int fd)
{
    static DIR dirs[64];
    static int next = 0;

    dirs[next].fd = fd;
    dirs[next].bpos = 0;

    return dirs + next++;
}

struct kernel_dirent {
    ino_t d_ino; // inode number
    uint32_t d_off; // ignored
    uint16_t d_reclen; // length of this struct user_dirent
    char d_name[1]; // file name with a padding zero
    // uint8_t d_type; // file type, with offset of (d_reclen - 1)
};

size_t fill_dirent(struct dirent* dent, char* buffer, size_t bpos)
{
    struct kernel_dirent* dp = (struct kernel_dirent*)(buffer + bpos);
    dent->d_ino = dp->d_ino;
    dent->d_off = dp->d_off;
    dent->d_reclen = dp->d_reclen;
    dent->d_type = buffer[bpos + dp->d_reclen - 1];
    strncpy(dent->d_name, dp->d_name, sizeof(dent->d_name));
    dent->d_name[sizeof(dent->d_name) - 1] = 0;

    return bpos + dp->d_reclen;
}

struct dirent* readdir(DIR* dirp)
{
    if (dirp->bpos) {
        if (dirp->bpos >= dirp->blen) {
            dirp->bpos = 0;
        } else {
            goto fill;
        }
    }

    dirp->blen = syscall3(SYS_getdents,
        dirp->fd,
        (uint32_t)dirp->buffer,
        sizeof(dirp->buffer)
    );

    if (dirp->blen <= 0)
        return NULL;

fill:
    dirp->bpos = fill_dirent(&dirp->dent, dirp->buffer, dirp->bpos);
    return &dirp->dent;
}
