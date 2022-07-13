#include "basic-lib.h"

int main(void)
{
    const char* data = "Hello World from user space init\n";
    syscall(0x01, (uint32_t)data, 0);
    for (;;) ;
    return 0;
}
