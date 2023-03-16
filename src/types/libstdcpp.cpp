#include <asm/port_io.h>
#include <assert.h>
#include <kernel/log.hpp>
#include <stdio.h>
#include <types/types.h>

void operator delete(void*)
{
    assert(false);
}
void operator delete(void*, unsigned int)
{
    assert(false);
}

extern "C" void NORETURN __stack_chk_fail(void)
{
    assert(false);
}

extern "C" void NORETURN __cxa_pure_virtual(void)
{
    assert(false);
}

void NORETURN
__assert_fail(const char* statement, const char* file, int line, const char* func)
{
    char buf[256];
    snprintf(buf, sizeof(buf), "Kernel assertion failed: (%s), %s:%d, %s\n",
        statement, file, line, func);
    kmsg(buf);
    asm_cli();
    asm_hlt();
    for (;;)
        ;
}
