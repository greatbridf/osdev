#include <stdarg.h>
#include <sys/ioctl.h>
#include <unistd.h>
#include <syscall.h>

ssize_t read(int fd, void* buf, size_t count)
{
    return syscall3(SYS_read, fd, (uint32_t)buf, count);
}

ssize_t write(int fd, const void* buf, size_t count)
{
    return syscall3(SYS_write, fd, (uint32_t)buf, count);
}

int dup(int oldfd)
{
    return syscall1(SYS_dup, oldfd);
}

int dup2(int oldfd, int newfd)
{
    return syscall2(SYS_dup2, oldfd, newfd);
}

int pipe(int pipefd[2])
{
    return syscall1(SYS_pipe, (uint32_t)pipefd);
}

int close(int fd)
{
    return syscall1(SYS_close, fd);
}

_Noreturn void _exit(int code)
{
    (void)syscall1(SYS_exit, code);
    // if syscall failed
    for (;;);
}

pid_t fork(void)
{
    return syscall0(SYS_fork);
}

int execve(const char* pathname, char* const argv[], char* const envp[])
{
    return syscall3(SYS_execve, (uint32_t)pathname, (uint32_t)argv, (uint32_t)envp);
}

unsigned int sleep(unsigned int seconds)
{
    return syscall1(SYS_sleep, seconds);
}

int chdir(const char* path)
{
    return syscall1(SYS_chdir, (uint32_t)path);
}

char* getcwd(char* buf, size_t bufsize)
{
    return (char*)syscall2(SYS_getcwd, (uint32_t)buf, bufsize);
}

pid_t getpid(void)
{
    return syscall0(SYS_getpid);
}

pid_t getppid(void)
{
    return syscall0(SYS_getppid);
}

int setpgid(pid_t pid, pid_t pgid)
{
    return syscall2(SYS_setpgid, pid, pgid);
}

pid_t setsid(void)
{
    return syscall0(SYS_setsid);
}

pid_t getsid(pid_t pid)
{
    return syscall1(SYS_getsid, pid);
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
    int ret = -1;

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
    return ret;
}
