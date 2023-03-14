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
