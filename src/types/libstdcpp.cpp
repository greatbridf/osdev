#include <assert.h>

#include <types/types.h>

#include <kernel/log.hpp>

extern "C" void NORETURN __stack_chk_fail(void) {
    assert(false);
    for (;;)
        ;
}

extern "C" void NORETURN __cxa_pure_virtual(void) {
    assert(false);
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
