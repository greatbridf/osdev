#include <assert.h>
#include <kernel/log.hpp>
#include <kernel/process.hpp>
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
    kmsgf("Kernel assertion failed: (%s), %s:%d, %s", statement, file, line, func);
    freeze();
}
