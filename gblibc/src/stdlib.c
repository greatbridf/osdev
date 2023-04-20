#include <stdlib.h>
#include <syscall.h>

int atoi(const char* str)
{
    int ret = 0;
    while (*str) {
        ret *= 10;
        ret += *str - '0';
    }
    return ret;
}

void __attribute__((noreturn)) exit(int status)
{
    syscall1(SYS_exit, status);
    for (;;)
        ;
}
