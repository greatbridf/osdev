#include <errno.h>
#include <sys/time.h>
#include <syscall.h>

int gettimeofday(struct timeval* tv, struct timezone* tz)
{
    if (tz) {
        errno = -EINVAL;
        return -1;
    }

    int ret = syscall2(SYS_gettimeofday, (uint32_t)tv, 0);

    if (ret < 0) {
        errno = -ret;
        return -1;
    }

    return ret;
}
