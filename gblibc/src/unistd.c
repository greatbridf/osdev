#include <errno.h>
#include <stdarg.h>
#include <stdint.h>
#include <sys/ioctl.h>
#include <unistd.h>
#include <syscall.h>

ssize_t read(int fd, void* buf, size_t count)
{
    ssize_t ret = syscall3(SYS_read, fd, (uint32_t)buf, count);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

ssize_t write(int fd, const void* buf, size_t count)
{
    ssize_t ret = syscall3(SYS_write, fd, (uint32_t)buf, count);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

int dup(int oldfd)
{
    int ret = syscall1(SYS_dup, oldfd);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

int dup2(int oldfd, int newfd)
{
    int ret = syscall2(SYS_dup2, oldfd, newfd);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

int pipe(int pipefd[2])
{
    int ret = syscall1(SYS_pipe, (uint32_t)pipefd);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

int close(int fd)
{
    int ret = syscall1(SYS_close, fd);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

_Noreturn void _exit(int code)
{
    (void)syscall1(SYS_exit, code);
    // if syscall failed
    for (;;);
}

pid_t fork(void)
{
    pid_t ret = syscall0(SYS_fork);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

int execve(const char* pathname, char* const argv[], char* const envp[])
{
    int ret = syscall3(SYS_execve, (uint32_t)pathname, (uint32_t)argv, (uint32_t)envp);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

unsigned int sleep(unsigned int seconds)
{
    return syscall1(SYS_sleep, seconds);
}

int chdir(const char* path)
{
    int ret = syscall1(SYS_chdir, (uint32_t)path);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

char* getcwd(char* buf, size_t bufsize)
{
    int ret = syscall2(SYS_getcwd, (uint32_t)buf, bufsize);
    if (ret < 0) {
        errno = -ret;
        return NULL;
    }
    return buf;
}

pid_t getpid(void)
{
    pid_t ret = syscall0(SYS_getpid);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

pid_t getppid(void)
{
    pid_t ret = syscall0(SYS_getppid);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

int setpgid(pid_t pid, pid_t pgid)
{
    int ret = syscall2(SYS_setpgid, pid, pgid);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

pid_t setsid(void)
{
    pid_t ret = syscall0(SYS_setsid);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

pid_t getsid(pid_t pid)
{
    pid_t ret = syscall1(SYS_getsid, pid);
    if (ret < 0) {
        errno = -ret;
        return -1;
    }
    return ret;
}

pid_t tcgetpgrp(int fd)
{
    pid_t pgrp;
    return ioctl(fd, TIOCGPGRP, &pgrp);
}

int tcsetpgrp(int fd, pid_t pgrp)
{
    return ioctl(fd, TIOCSPGRP, &pgrp);
}

int ioctl(int fd, unsigned long request, ...)
{
    int ret = -EINVAL;

    va_list args;
    va_start(args, request);

    switch (request) {
    case TIOCGPGRP:
        ret = syscall3(SYS_ioctl, fd, request, va_arg(args, uint32_t));
        break;
    case TIOCSPGRP:
        ret = syscall3(SYS_ioctl, fd, request, va_arg(args, uint32_t));
        break;
    default:
        break;
    }

    va_end(args);

    if (ret < 0) {
        errno = -ret;
        return -1;
    }

    return ret;
}
