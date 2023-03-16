#include <stdio.h>
#include <unistd.h>

_Noreturn void __attribute__((weak))
__assert_fail(const char* statement, const char* file, int line, const char* func)
{
    char buf[256] = {};
    int len = snprintf(buf, sizeof(buf), "Assertion failed: (%s) in %s:%d, %s\n",
        statement, file, line, func);
    write(STDERR_FILENO, buf, len);
    _exit(-1);
}
