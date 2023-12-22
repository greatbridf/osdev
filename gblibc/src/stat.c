#include <stdint.h>
#include <errno.h>
#include <syscall.h>
#include <sys/stat.h>

int stat(const char* pathname, struct stat* statbuf)
{
    int ret = syscall2(SYS_stat, (uint32_t)pathname, (uint32_t)statbuf);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

int fstat(int fd, struct stat* statbuf)
{
    int ret = syscall2(SYS_fstat, fd, (uint32_t)statbuf);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

mode_t umask(mode_t mask)
{
    return syscall1(SYS_umask, mask);
}
