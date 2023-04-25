#include <errno.h>
#include <syscall.h>
#include <sys/stat.h>

int fstat(int fd, struct stat* statbuf)
{
    int ret = syscall2(SYS_fstat, fd, (uint32_t)statbuf);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}
