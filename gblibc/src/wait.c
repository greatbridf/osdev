#include <sys/wait.h>
#include <syscall.h>

pid_t waitpid(pid_t pid, int* code, int options)
{
    return syscall3(SYS_waitpid, (uint32_t)pid, (uint32_t)code, (uint32_t)options);
}

pid_t wait(int* code)
{
    return waitpid(-1, code, 0);
}
