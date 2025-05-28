#include <assert.h>

#include <types/types.h>

extern "C" void NORETURN __stack_chk_fail(void) {
    for (;;)
        ;
}

extern "C" void NORETURN __cxa_pure_virtual(void) {
    for (;;)
        ;
}

void NORETURN __assert_fail(const char* statement, const char* file, int line, const char* func) {
    (void)statement, (void)file, (void)line, (void)func;
    for (;;)
        asm volatile(
            "cli\n\t"
            "hlt\n\t");
}
