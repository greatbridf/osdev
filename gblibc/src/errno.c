#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int* __errno_location(void)
{
    static int __errno = 0;
    return &__errno;
}

void
__attribute__((noreturn))
__attribute__((visibility("hidden")))
__stack_chk_fail_local(void)
{
    const char* msg = "***** stack overflow detected *****\n"
                      "quiting...\n";
    write(STDERR_FILENO, msg, strlen(msg));
    exit(-1);
}
