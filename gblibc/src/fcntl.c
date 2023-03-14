#include <fcntl.h>
#include <syscall.h>

int open(const char* filename, int flags, ...)
{
    return syscall2(SYS_open, (uint32_t)filename, flags);
}
