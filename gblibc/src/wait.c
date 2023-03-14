#include <sys/wait.h>
#include <syscall.h>

pid_t wait(int* code)
{
    return syscall1(SYS_wait, (uint32_t)code);
}
