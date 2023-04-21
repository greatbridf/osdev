#include <errno.h>
#include <sys/wait.h>
#include <syscall.h>

pid_t wait(int* code)
{
    int ret = syscall1(SYS_wait, (uint32_t)code);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}
