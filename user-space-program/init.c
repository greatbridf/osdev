#include "basic-lib.h"

int main(void)
{
    const char* data = "Hello World from user space init\n";
    syscall(0x01, (uint32_t)data, 0);
    int ret = syscall(0x00, 0, 0);
    if (ret == 0) {
        const char* child = "child\n";
        // write
        syscall(0x01, (uint32_t)child, 0);
        // exit
        syscall(0x05, 255, 0);
    } else {
        const char* parent = "parent\n";
        // write
        syscall(0x01, (uint32_t)parent, 0);
        for (;;) ;
    }
    return 0;
}
