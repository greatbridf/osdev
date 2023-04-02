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
    return syscall3(SYS_exec, (uint32_t)pathname, (uint32_t)argv, (uint32_t)envp);
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

pid_t setsid(void)
{
    return syscall0(SYS_setsid);
}

pid_t getsid(pid_t pid)
{
    return syscall1(SYS_getsid, pid);
}
