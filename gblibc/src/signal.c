#include <syscall.h>
#include <errno.h>
#include <unistd.h>
#include <signal.h>

int kill(pid_t pid, int sig)
{
    int ret = syscall2(SYS_kill, pid, sig);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

int raise(int sig)
{
    pid_t pid = getpid();
    if (pid < 0)
        return -1;

    return kill(pid, sig);
}
