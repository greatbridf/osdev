#include <stdarg.h>
#include <errno.h>
#include <fcntl.h>
#include <syscall.h>

#include <sys/types.h>

int open(const char* filename, int flags, ...)
{
    int ret;
    if (flags | O_CREAT) {
        va_list vl;
        va_start(vl, flags);

        ret = syscall3(SYS_open, (uint32_t)filename, flags, va_arg(vl, mode_t));

        va_end(vl);
    }
    else
        ret = syscall2(SYS_open, (uint32_t)filename, flags);

    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}
