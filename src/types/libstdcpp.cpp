#include <assert.h>
#include <kernel/log.hpp>
#include <kernel/process.hpp>
#include <stdio.h>
#include <types/types.h>

extern "C" void NORETURN __stack_chk_fail(void)
{
    assert(false);
    for (;;) ;
}

extern "C" void NORETURN __cxa_pure_virtual(void)
{
    assert(false);
    for (;;) ;
}

void NORETURN
__assert_fail(const char* statement, const char* file, int line, const char* func)
{
    char buf[256];
    snprintf(buf, sizeof(buf), "Kernel assertion failed: (%s), %s:%d, %s\n",
        statement, file, line, func);
    kmsg(buf);
    freeze();
}
