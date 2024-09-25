#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int* __errno_location(void)
{
    static int __errno = 0;
    return &__errno;
}

static size_t _strlen(const char* str)
{
    size_t len = 0;
    while (str[len] != '\0') {
        len++;
    }
    return len;
}

void
__attribute__((noreturn))
__attribute__((weak))
__stack_chk_fail(void)
{
    const char* msg = "***** stack overflow detected *****\n"
                      "quiting...\n";
    write(STDERR_FILENO, msg, _strlen(msg));
    exit(-1);
}

void
__attribute__((noreturn))
__attribute__((weak))
__stack_chk_fail_local(void)
{
    __stack_chk_fail();
}
