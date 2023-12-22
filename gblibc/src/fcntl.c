#include <errno.h>
#include <fcntl.h>
#include <syscall.h>

int open(const char* filename, int flags, ...)
{
    int ret = syscall2(SYS_open, (uint32_t)filename, flags);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}
